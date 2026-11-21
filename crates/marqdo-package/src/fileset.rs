//! Deterministic package file-set selection, hashing and materialization.
//!
//! `content_profile_version` 1 defines one selection used by both hashing and
//! copying. Hashing and copying must never disagree: a digest that does not
//! describe the materialized environment is not a reproducibility gate.
//!
//! Normative design: `docs/adr/0016-lock-v3-graph-and-package-file-set.md` §4.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

/// Version of the file-set selection and digest encoding.
pub const CONTENT_PROFILE_VERSION: u32 = 1;
/// Maximum number of files in one package.
pub const MAX_FILES: usize = 10_000;
/// Maximum size of one file.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum total size of one package.
pub const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum directory depth below the package root.
pub const MAX_DEPTH: usize = 64;

/// One regular file selected from a package tree.
#[derive(Debug, Clone)]
pub(crate) struct FileSetEntry {
    pub(crate) relative: String,
    pub(crate) path: PathBuf,
}

fn excluded_component(name: &OsStr) -> bool {
    name == OsStr::new(".git") || name == OsStr::new(".marqdo")
}

fn normalize_relative(relative: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| {
                        format!("package path `{}` is not valid UTF-8", relative.display())
                    })?
                    .to_string();
                if part.is_empty() {
                    return Err("package path contains an empty component".to_string());
                }
                parts.push(part);
            }
            _ => {
                return Err(format!(
                    "package path `{}` is absolute or contains `.`/`..`",
                    relative.display()
                ))
            }
        }
    }
    if parts.is_empty() {
        return Err("package path is empty".to_string());
    }
    Ok(parts.join("/"))
}

/// Select the regular files of a package deterministically.
///
/// Excludes any path component `.git` or `.marqdo`; includes `target/`.
/// Symlinks and special files are hard errors, never skipped or followed.
pub(crate) fn collect_file_set(root: &Path) -> Result<Vec<FileSetEntry>, String> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("cannot inspect package `{}`: {error}", root.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "package root `{}` is a symlink; refusing to hash or materialize it",
            root.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "package source `{}` is not a directory",
            root.display()
        ));
    }

    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !(entry.file_type().is_dir() && excluded_component(entry.file_name()))
        });

    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total: u64 = 0;
    for entry in walker {
        let entry = entry.map_err(|error| {
            let path = error
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| root.display().to_string());
            format!("cannot walk package `{path}`: {error}")
        })?;
        if entry.depth() == 0 {
            continue;
        }
        if entry.depth() > MAX_DEPTH {
            return Err(format!(
                "package `{}` exceeds the maximum directory depth of {MAX_DEPTH}",
                root.display()
            ));
        }
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "package contains a symlink `{}`; refusing to hash or materialize it",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(format!(
                "package contains special file `{}`; only regular files are package content",
                entry.path().display()
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| format!("package path `{}` escaped its root", entry.path().display()))?;
        let relative = normalize_relative(relative)?;
        if !seen.insert(relative.clone()) {
            return Err(format!(
                "package `{}` contains duplicate normalized path `{relative}`",
                root.display()
            ));
        }
        let file_metadata = entry.metadata().map_err(|error| {
            format!(
                "cannot inspect package file `{}`: {error}",
                entry.path().display()
            )
        })?;
        let len = file_metadata.len();
        if len > MAX_FILE_BYTES {
            return Err(format!(
                "package file `{}` is {len} bytes; maximum is {MAX_FILE_BYTES}",
                entry.path().display()
            ));
        }
        total = total
            .checked_add(len)
            .ok_or_else(|| "package size overflowed".to_string())?;
        if total > MAX_TOTAL_BYTES {
            return Err(format!(
                "package `{}` exceeds the maximum total size of {MAX_TOTAL_BYTES} bytes",
                root.display()
            ));
        }
        if files.len() >= MAX_FILES {
            return Err(format!(
                "package `{}` exceeds the maximum file count of {MAX_FILES}",
                root.display()
            ));
        }
        files.push(FileSetEntry {
            relative,
            path: entry.path().to_path_buf(),
        });
    }
    files.sort_by(|a, b| a.relative.as_bytes().cmp(b.relative.as_bytes()));
    Ok(files)
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    Ok(bytes)
}

fn update_digest(hasher: &mut Sha256, relative: &str, bytes: &[u8]) {
    hasher.update((relative.len() as u64).to_le_bytes());
    hasher.update(relative.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Hash a package directory with the profile-v1 file-set encoding.
pub(crate) fn hash_tree(root: &Path) -> Result<String, String> {
    let files = collect_file_set(root)?;
    let mut hasher = Sha256::new();
    hasher.update(b"marqdo.package.file-set\0");
    hasher.update(CONTENT_PROFILE_VERSION.to_le_bytes());
    hasher.update((files.len() as u64).to_le_bytes());
    for file in &files {
        let bytes = read_file(&file.path)?;
        update_digest(&mut hasher, &file.relative, &bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Materialize a package and verify that the written bytes match `expected`.
///
/// The source is read once: the same bytes are hashed and written, so a
/// source mutation during materialization is detected before publication.
/// Callers materialize into staging and publish only on success.
pub(crate) fn materialize_file_set(
    source: &Path,
    target: &Path,
    expected_hex: &str,
) -> Result<(), String> {
    let files = collect_file_set(source)?;
    if target.exists() {
        std::fs::remove_dir_all(target)
            .map_err(|error| format!("cannot reset `{}`: {error}", target.display()))?;
    }
    std::fs::create_dir_all(target)
        .map_err(|error| format!("cannot create `{}`: {error}", target.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(b"marqdo.package.file-set\0");
    hasher.update(CONTENT_PROFILE_VERSION.to_le_bytes());
    hasher.update((files.len() as u64).to_le_bytes());
    for file in &files {
        let bytes = read_file(&file.path)?;
        update_digest(&mut hasher, &file.relative, &bytes);
        let destination = target.join(&file.relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
        }
        std::fs::write(&destination, &bytes)
            .map_err(|error| format!("cannot write `{}`: {error}", destination.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o644));
        }
    }

    let actual = hex::encode(hasher.finalize());
    if actual != expected_hex {
        return Err(format!(
            "package `{}` changed while materializing (expected digest {expected_hex}, got {actual}); run `marqdo lock`",
            source.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn excludes_git_and_marqdo_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("index.mq.md"), b"## @exec main\n");
        write(&root.join(".git/HEAD"), b"ref: main\n");
        write(&root.join(".marqdo/env/env.json"), b"{}");
        write(&root.join("nested/.marqdo/state"), b"x");
        let first = hash_tree(root).unwrap();

        write(&root.join(".git/HEAD"), b"ref: other\n");
        write(&root.join(".marqdo/env/env.json"), b"{\"changed\":true}");
        assert_eq!(hash_tree(root).unwrap(), first);
    }

    #[test]
    fn target_directory_is_package_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("index.mq.md"), b"x");
        let first = hash_tree(root).unwrap();
        write(&root.join("target/generated"), b"y");
        assert_ne!(hash_tree(root).unwrap(), first);
    }

    #[test]
    fn rejects_symlinks() {
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            write(&root.join("index.mq.md"), b"x");
            std::os::unix::fs::symlink("index.mq.md", root.join("link.mq.md")).unwrap();
            let error = hash_tree(root).unwrap_err();
            assert!(error.contains("symlink"), "{error}");
        }
    }

    #[test]
    fn rejects_special_files() {
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            write(&root.join("index.mq.md"), b"x");
            let status = std::process::Command::new("mkfifo")
                .arg(root.join("pipe"))
                .status();
            if status.map(|status| status.success()).unwrap_or(false) {
                let error = hash_tree(root).unwrap_err();
                assert!(error.contains("special file"), "{error}");
            }
        }
    }

    #[test]
    fn digest_encoding_is_unambiguous() {
        // The v1 delimiter-only encoding produced the same stream for both
        // trees: `a\0x\0y\0z\0`. Length prefixes must distinguish them.
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        write(&first.join("a"), b"x\0y");
        write(&first.join("z"), b"");
        write(&second.join("a"), b"x");
        write(&second.join("y"), b"z");
        assert_ne!(hash_tree(&first).unwrap(), hash_tree(&second).unwrap());
    }

    #[test]
    fn materialization_uses_the_same_selection_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        write(&source.join("index.mq.md"), b"## @exec main\n");
        write(&source.join("data/value.txt"), b"42");
        write(&source.join(".git/HEAD"), b"ignored");
        let digest = hash_tree(&source).unwrap();
        materialize_file_set(&source, &target, &digest).unwrap();
        assert!(target.join("index.mq.md").is_file());
        assert!(target.join("data/value.txt").is_file());
        assert!(!target.join(".git").exists());
        assert_eq!(hash_tree(&target).unwrap(), digest);
    }

    #[test]
    fn materialization_rejects_a_wrong_expected_digest() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        write(&source.join("index.mq.md"), b"x");
        let error = materialize_file_set(&source, &target, "0000").unwrap_err();
        assert!(error.contains("changed while materializing"), "{error}");
    }
}
