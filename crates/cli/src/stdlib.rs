//! Bundled standard library discovery, digest verification and `std:` imports.
//!
//! The official CLI ships a fixed `marqdo-stdlib` tree (ADR-0026/CM-A10). This
//! module finds that tree, verifies its declared content digest before any
//! module is loaded, and resolves explicit `std:<relative>` imports to it.
//! Nothing is fetched implicitly and there is no second module search path.

use std::path::{Component, Path, PathBuf};

use marqdo_package::{PackageOrigin, ResolvedModule, ResolvedPackage};
use sha2::{Digest, Sha256};

/// Environment override used by tests and relocatable installations.
pub const STDLIB_DIR_ENV: &str = "MARQDO_STDLIB_DIR";

/// Verified bundled standard library.
#[derive(Debug, Clone)]
pub struct StdlibBundle {
    root: PathBuf,
    name: String,
    version: String,
    digest: String,
}

impl StdlibBundle {
    /// Discover the bundle.
    ///
    /// Order: `MARQDO_STDLIB_DIR` (explicit, fail if invalid), a `stdlib`
    /// directory next to the executable, then the development tree.
    pub fn discover() -> Result<Option<Self>, String> {
        if let Some(dir) = std::env::var_os(STDLIB_DIR_ENV) {
            return Self::load(PathBuf::from(dir)).map(Some);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate = parent.join("stdlib");
                if candidate.is_dir() {
                    return Self::load(candidate).map(Some);
                }
            }
        }
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("stdlib");
        if dev.is_dir() {
            return Self::load(dev).map(Some);
        }
        Ok(None)
    }

    /// Load and verify one bundle directory.
    pub fn load(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        let root = root.canonicalize().map_err(|error| {
            format!("cannot open stdlib directory `{}`: {error}", root.display())
        })?;
        let manifest_path = root.join("stdlib.json");
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|error| format!("cannot read `{}`: {error}", manifest_path.display()))?;
        let manifest: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("invalid `{}`: {error}", manifest_path.display()))?;
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("`{}` has no name", manifest_path.display()))?
            .to_string();
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("`{}` has no version", manifest_path.display()))?
            .to_string();
        let declared = manifest
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("`{}` has no digest", manifest_path.display()))?
            .to_string();
        let actual = tree_digest(&root)?;
        if declared != actual {
            return Err(format!(
                "stdlib digest mismatch: declared {declared}, actual {actual}"
            ));
        }
        Ok(Self {
            root,
            name,
            version,
            digest: actual,
        })
    }

    /// Declared stdlib version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Verified content digest (`sha256:<hex>`).
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Logical package identity used for module ids.
    pub fn instance_id(&self) -> String {
        format!(
            "std:{}@{}#{}",
            self.name,
            self.version,
            self.digest.trim_start_matches("sha256:")
        )
    }

    /// Package view consumed by the loader.
    pub fn package(&self) -> ResolvedPackage {
        ResolvedPackage {
            instance_id: self.instance_id(),
            name: self.name.clone(),
            version: self.version.clone(),
            origin: PackageOrigin::Path {
                declared: PathBuf::from("stdlib"),
            },
            revision: None,
            content_digest: Some(self.digest.clone()),
            module_root: self.root.clone(),
            entry_module: "index.mq.md".to_string(),
            manifest: None,
        }
    }

    /// Resolve a `std:<relative>` target inside the bundle.
    pub fn resolve(&self, target: &str) -> Result<ResolvedModule, String> {
        let mut normalised = PathBuf::new();
        for component in Path::new(target).components() {
            match component {
                Component::Normal(part) => normalised.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(format!(
                        "stdlib import `{target}` must be a relative path inside the stdlib"
                    ))
                }
            }
        }
        if normalised.as_os_str().is_empty() {
            return Err("stdlib import must name a module".to_string());
        }
        if normalised.extension().is_none() {
            normalised.set_extension("mq.md");
        }
        let candidate = self.root.join(&normalised);
        let canonical = candidate.canonicalize().map_err(|error| {
            format!(
                "cannot open stdlib module `{target}` at `{}`: {error}",
                candidate.display()
            )
        })?;
        let relative = canonical
            .strip_prefix(&self.root)
            .map_err(|_| format!("stdlib import `{target}` escapes the stdlib root"))?;
        let relative_path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        Ok(ResolvedModule {
            package: self.package(),
            relative_path,
        })
    }
}

/// Content digest over all `.mq.md` files: sorted path, NUL, bytes, NUL.
pub fn tree_digest(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_markdown(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

fn collect_markdown(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(root, &path, out)?;
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".mq.md"))
        {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "stdlib file escaped its root".to_string())?
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            out.push((relative, path));
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_bundle() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("stdlib")
    }

    #[test]
    fn dev_bundle_verifies_and_resolves() {
        let bundle = StdlibBundle::load(dev_bundle()).unwrap();
        assert!(bundle.digest.starts_with("sha256:"));
        let io = bundle.resolve("io").unwrap();
        assert_eq!(io.relative_path, "io.mq.md");
        assert!(io.package.instance_id.starts_with("std:marqdo-stdlib@"));
        assert!(bundle.resolve("../escape").is_err());
    }

    #[test]
    fn digest_tamper_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dev_bundle();
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
            }
        }
        std::fs::write(dir.path().join("io.mq.md"), "## tampered\n").unwrap();
        let error = StdlibBundle::load(dir.path()).unwrap_err();
        assert!(error.contains("digest mismatch"), "{error}");
    }
}
