//! Package lock/sync/add/remove commands.

use super::*;
use crate::extension_selection;

/// `marqdo lock`.
pub fn lock(json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest = match marqdo_package::Manifest::load(&root.join(marqdo_package::MANIFEST_FILE)) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let resolver = marqdo_package::PackageResolver::new(root.clone(), Some(manifest.clone()), None)
        .with_fetch(true)
        .with_unlocked(true);
    let graph = match resolver.resolve_lock_graph() {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    let mut lockfile = graph.lock;
    // Lock v4: record the exact extension identity declared by the project.
    // Reading manifests/declared digests is static; no binary is loaded and no
    // grant is implied.
    let extension_sources = match resolver.resolve_extension_sources() {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: extension selection: {error}");
            return 1;
        }
    };
    let identities = match extension_selection::extension_identities(&extension_sources) {
        Ok(identities) => identities,
        Err(error) => {
            eprintln!("error[{}]: {}", error.code, error.message);
            return error.exit_code;
        }
    };
    for identity in identities {
        if let Err(error) = lockfile.merge_extension(identity) {
            eprintln!("error[ext.extension_lock_mismatch]: {error}");
            return 1;
        }
    }
    let path = root.join(marqdo_package::LOCK_FILE);
    if let Err(error) = write_atomic(&path, lockfile.to_yaml().as_bytes()) {
        eprintln!("error: cannot write {}: {error}", path.display());
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "lock",
            "success": true,
            "packages": lockfile.packages,
            "extensions": lockfile.extensions,
        }));
    } else {
        println!(
            "locked {} package(s) in {}",
            lockfile.packages.len(),
            path.display()
        );
    }
    0
}

/// `marqdo sync [--locked]`.
pub fn sync(locked: bool, json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest = match marqdo_package::Manifest::load(&root.join(marqdo_package::MANIFEST_FILE)) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let lock_path = root.join(marqdo_package::LOCK_FILE);
    if !lock_path.is_file() {
        eprintln!(
            "error: {} is missing; run `marqdo lock` first",
            lock_path.display()
        );
        return 1;
    }
    let lockfile = match marqdo_package::LockFile::load(&lock_path) {
        Ok(lockfile) => lockfile,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    if let Err(error) = lockfile.validate_root(&manifest) {
        eprintln!("error: {error}");
        return 1;
    }
    let resolver =
        marqdo_package::PackageResolver::new(root, Some(manifest.clone()), Some(lockfile.clone()))
            .with_fetch(!locked);
    // Verify the declared extension selection against the lock before using
    // any materialized content. A missing/changed identity fails closed.
    let extension_sources = match resolver.resolve_extension_sources() {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("error: extension selection: {error}");
            return 1;
        }
    };
    let identities = match extension_selection::extension_identities(&extension_sources) {
        Ok(identities) => identities,
        Err(error) => {
            eprintln!("error[{}]: {}", error.code, error.message);
            return error.exit_code;
        }
    };
    if let Err(error) = lockfile.validate_extension_declarations(&manifest) {
        eprintln!("error[ext.extension_lock_mismatch]: {error}");
        return 1;
    }
    if let Err(error) = extension_selection::validate_against_lock(&lockfile, &identities) {
        eprintln!("error[{}]: {}", error.code, error.message);
        return error.exit_code;
    }
    if let Err(error) = resolver.materialize_env(&lockfile) {
        eprintln!("error: cannot materialize project environment: {error}");
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "sync",
            "success": true,
            "locked": locked,
            "packages": lockfile.packages,
        }));
    } else {
        println!(
            "synchronized {} package(s){} into {}",
            lockfile.packages.len(),
            if locked { " (locked)" } else { "" },
            resolver.env_root().display()
        );
    }
    0
}

fn discover_project_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    marqdo_package::discover_root(&cwd).ok_or_else(|| {
        format!(
            "no {} found in {} or any parent",
            marqdo_package::MANIFEST_FILE,
            cwd.display()
        )
    })
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "marqdo.lock".to_string());
    let temp = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&temp)
            .map_err(|error| format!("cannot create {}: {error}", temp.display()))?;
        std::io::Write::write_all(&mut file, contents)
            .map_err(|error| format!("cannot write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync {}: {error}", temp.display()))?;
        std::fs::rename(&temp, path)
            .map_err(|error| format!("cannot publish {}: {error}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// `marqdo add <name> --path <dir> [--git URL --rev SHA]`.
pub fn add(
    name: &str,
    path: Option<&Path>,
    git: Option<&str>,
    rev: Option<&str>,
    json: bool,
) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest_path = root.join(marqdo_package::MANIFEST_FILE);
    let mut manifest = match marqdo_package::Manifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let dependency = marqdo_package::Dependency {
        path: path.map(|path| path.to_path_buf()),
        git: git.map(str::to_string),
        rev: rev.map(str::to_string),
    };
    if let Err(error) = dependency.validate(name) {
        eprintln!("error: {error}");
        return 2;
    }
    if manifest.dependencies.contains_key(name) {
        eprintln!("error: dependency `{name}` already exists; remove it first");
        return 1;
    }
    manifest.dependencies.insert(name.to_string(), dependency);
    if let Err(error) = std::fs::write(&manifest_path, manifest.to_yaml()) {
        eprintln!("error: cannot write {}: {error}", manifest_path.display());
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "add",
            "success": true,
            "dependency": name,
            "manifest": manifest_path.display().to_string(),
        }));
    } else {
        println!("added dependency `{name}` to {}", manifest_path.display());
        println!(
            "run `marqdo lock` to pin it in {}",
            marqdo_package::LOCK_FILE
        );
    }
    0
}

/// `marqdo remove <name>`.
pub fn remove(name: &str, json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest_path = root.join(marqdo_package::MANIFEST_FILE);
    let mut manifest = match marqdo_package::Manifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    if manifest.dependencies.remove(name).is_none() {
        eprintln!("error: dependency `{name}` is not declared");
        return 1;
    }
    if let Err(error) = std::fs::write(&manifest_path, manifest.to_yaml()) {
        eprintln!("error: cannot write {}: {error}", manifest_path.display());
        return 1;
    }
    // The lock is a graph, not a per-name list: regenerating it here would
    // require resolving the new manifest. Leave it stale and tell the user.
    if json {
        output::print_json(&json!({
            "command": "remove",
            "success": true,
            "dependency": name,
        }));
    } else {
        println!("removed dependency `{name}`");
        println!("run `marqdo lock` to refresh {}", marqdo_package::LOCK_FILE);
    }
    0
}
