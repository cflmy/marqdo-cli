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

/// One official bundled-stdlib public alias (ADR-0058).
///
/// The alias table lives in the verified `stdlib.json` metadata, not in Rust
/// source: `打印`/`print` are ordinary `Marqdo` bindings to `io.mq.md::写值`
/// and the runtime no longer special-cases print names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicAlias {
    /// Public names that resolve to the target function.
    pub public_names: Vec<String>,
    /// Package-relative target module (for example `io.mq.md`).
    pub target_module: String,
    /// Canonical/local target function name.
    pub target_function: String,
    /// Declared parameter names, used to verify the target signature.
    pub expected_signature: Option<String>,
    /// Conservative effect request summary.
    pub effects: Vec<String>,
}

/// Verified bundled standard library.
#[derive(Debug, Clone)]
pub struct StdlibBundle {
    root: PathBuf,
    name: String,
    version: String,
    digest: String,
    aliases: Vec<PublicAlias>,
}

impl StdlibBundle {
    /// Discover the bundle.
    ///
    /// Order: `MARQDO_STDLIB_DIR` (explicit, fail if invalid) then a `stdlib`
    /// directory next to the executable. The official Bundle is an installed
    /// artifact next to the binary (RFC-EXT-0003 H-08, ADR-0070); there is no
    /// compile-time source-tree fallback in any build profile.
    pub fn discover() -> Result<Option<Self>, String> {
        let override_dir = std::env::var_os(STDLIB_DIR_ENV).map(PathBuf::from);
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::discover_with(override_dir, exe_dir)
    }

    /// Discovery with the explicit override and the executable directory passed
    /// in. The override is authoritative: a bad `MARQDO_STDLIB_DIR` fails even
    /// when a valid sibling Bundle exists (no silent fallback).
    fn discover_with(
        override_dir: Option<PathBuf>,
        exe_dir: Option<PathBuf>,
    ) -> Result<Option<Self>, String> {
        if let Some(dir) = override_dir {
            return Self::load(dir).map(Some);
        }
        Self::discover_from_exe_dir(exe_dir)
    }

    /// Discover from the directory that holds the executable, without the explicit
    /// environment override. Only the installed sibling Bundle is considered.
    fn discover_from_exe_dir(exe_dir: Option<PathBuf>) -> Result<Option<Self>, String> {
        if let Some(parent) = exe_dir {
            let candidate = parent.join("stdlib");
            if candidate.is_dir() {
                return Self::load(candidate).map(Some);
            }
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
        let aliases = parse_public_aliases(&manifest, &manifest_path)?;
        let declared_alias_digest = manifest
            .get("aliases_digest")
            .and_then(serde_json::Value::as_str);
        let actual_alias_digest = aliases_digest(&aliases);
        match (declared_alias_digest, aliases.is_empty()) {
            (Some(declared), _) => {
                if declared != actual_alias_digest {
                    return Err(format!(
                        "stdlib public-alias metadata digest mismatch: declared {declared}, \
                         actual {actual_alias_digest}"
                    ));
                }
            }
            (None, false) => {
                return Err(format!(
                    "`{}` declares public_aliases without aliases_digest",
                    manifest_path.display()
                ));
            }
            (None, true) => {}
        }
        Ok(Self {
            root,
            name,
            version,
            digest: actual,
            aliases,
        })
    }

    /// Verified official public aliases.
    pub fn public_aliases(&self) -> &[PublicAlias] {
        &self.aliases
    }

    /// Bundle root.
    pub fn root(&self) -> &Path {
        &self.root
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

fn parse_public_aliases(
    manifest: &serde_json::Value,
    manifest_path: &Path,
) -> Result<Vec<PublicAlias>, String> {
    let Some(entries) = manifest.get("public_aliases") else {
        return Ok(Vec::new());
    };
    let entries = entries.as_array().ok_or_else(|| {
        format!(
            "`{}` public_aliases must be an array",
            manifest_path.display()
        )
    })?;
    let mut aliases = Vec::new();
    let mut seen_names = std::collections::BTreeSet::new();
    for entry in entries {
        let public_names = entry
            .get("public_names")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                format!(
                    "`{}` public alias has no public_names array",
                    manifest_path.display()
                )
            })?
            .iter()
            .map(|name| {
                name.as_str().map(ToString::to_string).ok_or_else(|| {
                    format!(
                        "`{}` public alias name must be a string",
                        manifest_path.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if public_names.is_empty() {
            return Err(format!(
                "`{}` public alias declares no names",
                manifest_path.display()
            ));
        }
        for name in &public_names {
            if name.is_empty() {
                return Err(format!(
                    "`{}` public alias name must not be empty",
                    manifest_path.display()
                ));
            }
            if !seen_names.insert(name.clone()) {
                return Err(format!(
                    "`{}` public alias name `{name}` is declared more than once",
                    manifest_path.display()
                ));
            }
        }
        let target_module = entry
            .get("target_module")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "`{}` public alias has no target_module",
                    manifest_path.display()
                )
            })?
            .to_string();
        validate_bundle_relative(&target_module).map_err(|error| {
            format!(
                "`{}` public alias target_module `{target_module}` is invalid: {error}",
                manifest_path.display()
            )
        })?;
        let target_function = entry
            .get("target_function")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "`{}` public alias has no target_function",
                    manifest_path.display()
                )
            })?
            .to_string();
        if target_function.is_empty() {
            return Err(format!(
                "`{}` public alias target_function must not be empty",
                manifest_path.display()
            ));
        }
        let expected_signature = entry
            .get("expected_signature")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let effects = entry
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .map(|effects| {
                effects
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        aliases.push(PublicAlias {
            public_names,
            target_module,
            target_function,
            expected_signature,
            effects,
        });
    }
    aliases.sort_by(|left, right| {
        (&left.target_module, &left.target_function)
            .cmp(&(&right.target_module, &right.target_function))
    });
    Ok(aliases)
}

fn validate_bundle_relative(path: &str) -> Result<(), String> {
    let mut normalised = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalised.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("must be a relative path inside the bundle".to_string())
            }
        }
    }
    if normalised.as_os_str().is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(())
}

/// Canonical digest over the official public-alias table.
///
/// Format (identical to `tools/pack-stdlib.py`): for every alias sorted by
/// `(target_module, target_function)`: module, NUL, function, NUL, then every
/// public name sorted, NUL. A fixed binary format avoids JSON formatting drift.
pub fn aliases_digest(aliases: &[PublicAlias]) -> String {
    let mut sorted = aliases.to_vec();
    sorted.sort_by(|left, right| {
        (&left.target_module, &left.target_function)
            .cmp(&(&right.target_module, &right.target_function))
    });
    let mut hasher = Sha256::new();
    for alias in &sorted {
        hasher.update(alias.target_module.as_bytes());
        hasher.update([0]);
        hasher.update(alias.target_function.as_bytes());
        hasher.update([0]);
        let mut names = alias.public_names.clone();
        names.sort();
        for name in names {
            hasher.update(name.as_bytes());
            hasher.update([0]);
        }
    }
    format!("sha256:{}", hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small synthetic Bundle so loader unit tests never depend on the
    /// product tree (ADR-0070 test-lane separation).
    fn write_synthetic_bundle(root: &Path) {
        for (relative, body) in [
            ("io.mq.md", "## @knowledge test\n"),
            ("io/console.mq.md", "## @knowledge console\n"),
        ] {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let digest = tree_digest(root).unwrap();
        let manifest = serde_json::json!({
            "name": "test-stdlib",
            "version": "0.0.0-test",
            "digest": digest,
        });
        std::fs::write(
            root.join("stdlib.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn synthetic_bundle_verifies_and_resolves() {
        let dir = tempfile::tempdir().unwrap();
        write_synthetic_bundle(dir.path());
        let bundle = StdlibBundle::load(dir.path()).unwrap();
        assert_eq!(bundle.version(), "0.0.0-test");
        let io = bundle.resolve("io").unwrap();
        assert_eq!(io.relative_path, "io.mq.md");
        assert!(io.package.instance_id.starts_with("std:test-stdlib@"));
        assert!(bundle.resolve("../escape").is_err());
        assert!(bundle.resolve("/absolute").is_err());
    }

    #[test]
    fn sibling_stdlib_is_found_without_environment_override() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("stdlib");
        std::fs::create_dir_all(&bundle).unwrap();
        write_synthetic_bundle(&bundle);
        let found = StdlibBundle::discover_from_exe_dir(Some(dir.path().to_path_buf()))
            .unwrap()
            .expect("sibling stdlib");
        assert_eq!(found.root(), bundle.canonicalize().unwrap());
    }

    #[test]
    fn bad_override_fails_closed_even_with_a_valid_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let sibling = dir.path().join("stdlib");
        std::fs::create_dir_all(&sibling).unwrap();
        write_synthetic_bundle(&sibling);
        let error = StdlibBundle::discover_with(
            Some(dir.path().join("missing")),
            Some(dir.path().to_path_buf()),
        )
        .unwrap_err();
        assert!(error.contains("cannot open stdlib directory"), "{error}");
    }

    #[test]
    fn digest_tamper_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write_synthetic_bundle(dir.path());
        std::fs::write(dir.path().join("io.mq.md"), "## @knowledge tampered\n").unwrap();
        let error = StdlibBundle::load(dir.path()).unwrap_err();
        assert!(error.contains("digest mismatch"), "{error}");
    }
}
