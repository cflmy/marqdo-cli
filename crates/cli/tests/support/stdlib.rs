//! Pinned stdlib Bundle resolver shared by test harnesses (ADR-0070).
//!
//! Product tests consume one explicit Bundle whose identity is pinned in
//! `tools/stdlib/bundle-pin.json` (name, version, source commit, tree digest and
//! alias digest). `MARQDO_TEST_STDLIB_BUNDLE` may change the location, but the
//! Bundle must still match the pin; `MARQDO_TEST_BUNDLE_PIN` may change the pin
//! file. A bad explicit input fails even when the default location is valid —
//! there is no silent fallback.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

pub const BUNDLE_ENV: &str = "MARQDO_TEST_STDLIB_BUNDLE";
pub const PIN_ENV: &str = "MARQDO_TEST_BUNDLE_PIN";

/// The checkout root (this test support module lives in `crates/cli/tests`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("repository root")
        .to_path_buf()
}

/// Pin file path, overridable for a candidate Bundle.
pub fn pin_path() -> PathBuf {
    std::env::var_os(PIN_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root()
                .join("tools")
                .join("stdlib")
                .join("bundle-pin.json")
        })
}

/// One pinned Bundle identity (`tools/stdlib/bundle-pin.json`).
#[derive(Debug, Clone)]
pub struct BundlePin {
    pub name: String,
    pub version: String,
    pub source_commit: String,
    /// None for the accepted ecosystem Bundle (Phase B): it is not vendored in
    /// this repository, so product tests require an explicit verified Bundle.
    pub source_path: Option<PathBuf>,
    pub tree_digest: String,
    pub aliases_digest: String,
}

impl BundlePin {
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read Bundle pin {}: {error}", path.display()));
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("invalid Bundle pin {}: {error}", path.display()));
        assert_eq!(
            value.get("schema").and_then(serde_json::Value::as_str),
            Some("marqdo.stdlib-bundle-pin"),
            "{} must use the marqdo.stdlib-bundle-pin schema",
            path.display()
        );
        assert_eq!(
            value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64),
            Some(1),
            "{} must use pin schema_version 1",
            path.display()
        );
        let source = value
            .get("source")
            .unwrap_or_else(|| panic!("Bundle pin {} has no source object", path.display()));
        let source_kind = source
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("in-repo");
        let source_path = match source_kind {
            "in-repo" => {
                let declared = source
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| {
                        panic!("in-repo Bundle pin {} has no source.path", path.display())
                    });
                validate_relative(declared).unwrap_or_else(|error| {
                    panic!("Bundle pin {} source.path: {error}", path.display())
                });
                Some(PathBuf::from(declared))
            }
            "ecosystem" => {
                for field in ["repository", "commit"] {
                    required_str(source, field, path);
                }
                None
            }
            other => panic!(
                "Bundle pin {} has unknown source.kind {other:?}",
                path.display()
            ),
        };
        Self {
            name: required_str(&value, "name", path),
            version: required_str(&value, "version", path),
            source_commit: required_str(source, "commit", path),
            source_path,
            tree_digest: required_str(&value, "tree_digest", path),
            aliases_digest: required_str(&value, "aliases_digest", path),
        }
    }

    /// The committed pin for the current checkout.
    pub fn committed() -> Self {
        Self::load(&pin_path())
    }
}

fn required_str(value: &serde_json::Value, field: &str, path: &Path) -> String {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| panic!("Bundle pin {} has no `{field}`", path.display()))
        .to_string()
}

/// One verified Bundle on disk.
#[derive(Debug, Clone)]
pub struct Bundle {
    root: PathBuf,
    pub name: String,
    pub version: String,
    pub tree_digest: String,
    pub aliases_digest: String,
}

impl Bundle {
    /// Resolve the Bundle for a product test: explicit override or pinned path,
    /// verified against the pin.
    pub fn pinned() -> Self {
        let pin = BundlePin::committed();
        let explicit = std::env::var_os(BUNDLE_ENV).map(PathBuf::from);
        let root = match (explicit.clone(), pin.source_path.as_ref()) {
            (Some(path), _) => path,
            (None, Some(relative)) => repo_root().join(relative),
            (None, None) => panic!(
                "the accepted stdlib Bundle is not vendored in this repository; set \
                 {BUNDLE_ENV} to a Bundle produced from the pinned ecosystem commit {} \
                 (tools/ci/fetch_ecosystem_bundle.py)",
                pin.source_commit
            ),
        };
        let bundle = Self::verify(&root).unwrap_or_else(|error| match &explicit {
            Some(_) => panic!(
                "{BUNDLE_ENV} does not name a valid stdlib Bundle at {}: {error}",
                root.display()
            ),
            None => panic!(
                "pinned stdlib Bundle at {} is invalid: {error}",
                root.display()
            ),
        });
        let mismatches = [
            ("name", &pin.name, &bundle.name),
            ("version", &pin.version, &bundle.version),
            ("tree_digest", &pin.tree_digest, &bundle.tree_digest),
            (
                "aliases_digest",
                &pin.aliases_digest,
                &bundle.aliases_digest,
            ),
        ];
        for (field, expected, actual) in mismatches {
            assert_eq!(
                expected,
                actual,
                "stdlib Bundle at {} does not match the pin field `{field}`",
                root.display()
            );
        }
        for required in ["stdlib.json", "io.mq.md", "io/console.mq.md"] {
            assert!(
                bundle.root.join(required).is_file(),
                "stdlib Bundle at {} is missing required file {required}",
                bundle.root.display()
            );
        }
        assert_eq!(
            pin.source_commit.len(),
            40,
            "the pin must record a full source commit, got {:?}",
            pin.source_commit
        );
        bundle
    }

    /// Verify one unpacked Bundle root.
    pub fn verify(root: &Path) -> Result<Self, String> {
        if !root.is_dir() {
            return Err(format!("not a directory: {}", root.display()));
        }
        let manifest_path = root.join("stdlib.json");
        if !manifest_path.is_file() {
            return Err(format!("missing manifest: {}", manifest_path.display()));
        }
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
        let manifest: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{} has no name", manifest_path.display()))?
            .to_string();
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{} has no version", manifest_path.display()))?
            .to_string();
        let declared = manifest
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{} has no digest", manifest_path.display()))?
            .to_string();
        let actual = tree_digest(root)?;
        if declared != actual {
            return Err(format!(
                "tree digest mismatch: stdlib.json declares {declared}, the files are {actual}"
            ));
        }
        let aliases = manifest
            .get("public_aliases")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let declared_aliases = manifest
            .get("aliases_digest")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let actual_aliases = aliases_digest(&aliases);
        match (declared_aliases, aliases.is_empty()) {
            (Some(declared_aliases), _) => {
                if declared_aliases != actual_aliases {
                    return Err(format!(
                        "alias digest mismatch: stdlib.json declares {declared_aliases}, \
                         the alias table is {actual_aliases}"
                    ));
                }
            }
            (None, false) => {
                return Err(format!(
                    "{} declares public_aliases without aliases_digest",
                    manifest_path.display()
                ));
            }
            (None, true) => {}
        }
        Ok(Self {
            root: root
                .canonicalize()
                .map_err(|error| format!("cannot canonicalize {}: {error}", root.display()))?,
            name,
            version,
            tree_digest: actual,
            aliases_digest: actual_aliases,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Resolve and verify the pinned Bundle once per test process.
pub fn bundle() -> &'static Bundle {
    static BUNDLE: std::sync::OnceLock<Bundle> = std::sync::OnceLock::new();
    BUNDLE.get_or_init(Bundle::pinned)
}

/// Bundle root for fixtures and `MARQDO_STDLIB_DIR`.
pub fn bundle_dir() -> PathBuf {
    bundle().root().to_path_buf()
}

/// Bundle root when one is configured, without failing tests that do not need
/// the standard library (Phase B has no in-repo source tree).
pub fn try_bundle_dir() -> Option<PathBuf> {
    let pin = BundlePin::committed();
    if let Some(explicit) = std::env::var_os(BUNDLE_ENV) {
        return Some(PathBuf::from(explicit));
    }
    pin.source_path.map(|relative| repo_root().join(relative))
}

/// A real CLI command that names the verified Bundle explicitly when one is
/// configured; spawned programs never rely on a compile-time source fallback.
pub fn cli_command() -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_marqdo"));
    if let Some(root) = try_bundle_dir() {
        command.env("MARQDO_STDLIB_DIR", root);
    }
    command
}

/// Content digest over all `.mq.md` files: sorted relative path, NUL, bytes, NUL.
pub fn tree_digest(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_markdown(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

/// Canonical digest over the official public-alias table.
pub fn aliases_digest(aliases: &[serde_json::Value]) -> String {
    let mut entries: Vec<&serde_json::Value> = aliases.iter().collect();
    entries.sort_by_key(|alias| {
        (
            alias["target_module"].as_str().unwrap_or("").to_string(),
            alias["target_function"].as_str().unwrap_or("").to_string(),
        )
    });
    let mut hasher = Sha256::new();
    for alias in entries {
        if let Some(module) = alias["target_module"].as_str() {
            hasher.update(module.as_bytes());
        }
        hasher.update([0]);
        if let Some(function) = alias["target_function"].as_str() {
            hasher.update(function.as_bytes());
        }
        hasher.update([0]);
        let mut names: Vec<&str> = alias["public_names"]
            .as_array()
            .map(|names| names.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        names.sort();
        for name in names {
            hasher.update(name.as_bytes());
            hasher.update([0]);
        }
    }
    format!("sha256:{}", hex(&hasher.finalize()))
}

fn collect_markdown(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
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

fn validate_relative(path: &str) -> Result<(), String> {
    let mut normalised = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalised.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("must be a relative path inside the repository".to_string())
            }
        }
    }
    if normalised.as_os_str().is_empty() {
        return Err("must not be empty".to_string());
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
