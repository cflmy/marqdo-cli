//! Project manifest, lockfile and dependency resolution.
//!
//! Blueprint §8: `marqdo.yaml` is the project manifest, `marqdo.lock` the
//! reproducible lockfile, and front matter imports are explicit document-level
//! bindings. This crate only parses and serialises these files; it never runs
//! project code and never executes install scripts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Manifest file name.
pub const MANIFEST_FILE: &str = "marqdo.yaml";
/// Lockfile name.
pub const LOCK_FILE: &str = "marqdo.lock";

/// Project identity and runtime constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Required Marqdo runtime range (informational in M0.5).
    #[serde(default)]
    pub marqdo: Option<String>,
}

/// One declared dependency source. Exactly one source form is allowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Dependency {
    /// Local path relative to the project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Git URL (must be pinned by `rev` for reproducible builds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// Exact git revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

impl Dependency {
    /// Validate that exactly one source is declared and that Git is pinned.
    pub fn validate(&self, name: &str) -> Result<(), String> {
        match (&self.path, &self.git) {
            (Some(_), Some(_)) => Err(format!(
                "dependency `{name}` declares both path and git; choose one"
            )),
            (None, None) => Err(format!(
                "dependency `{name}` must declare a path or a pinned git source"
            )),
            (None, Some(_)) if self.rev.is_none() => Err(format!(
                "dependency `{name}` uses git without an exact rev; pin the revision for reproducible builds"
            )),
            (Some(path), None) if path.is_absolute() => Err(format!(
                "dependency `{name}` path must be project-relative"
            )),
            _ => Ok(()),
        }
    }
}

/// Capability requests. Requests are not grants: the host decides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequests {
    /// File system requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs: Option<FsRequests>,
    /// Allowed network hosts (empty means none).
    #[serde(default)]
    pub net: Option<NetRequests>,
    /// May the project request process execution.
    #[serde(default)]
    pub process: Option<bool>,
}

/// File system capability requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRequests {
    /// Read globs.
    #[serde(default)]
    pub read: Vec<String>,
    /// Write globs.
    #[serde(default)]
    pub write: Vec<String>,
}

/// Network capability requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetRequests {
    /// Allowed hosts.
    #[serde(default)]
    pub hosts: Vec<String>,
}

/// The `marqdo.yaml` project manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Project identity.
    pub project: ProjectInfo,
    /// Dependencies keyed by local name.
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    /// Plugin requirements keyed by plugin name.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginRequirement>,
    /// Capability requests.
    #[serde(default)]
    pub capabilities: CapabilityRequests,
}

/// A requested plugin (installation is not authorization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRequirement {
    /// Requested version requirement.
    pub version: String,
}

impl Manifest {
    /// Parse a manifest from YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let manifest: Manifest = serde_yaml::from_str(yaml)
            .map_err(|error| format!("invalid {MANIFEST_FILE}: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Load a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::from_yaml(&yaml)
    }

    /// Validate manifest invariants.
    pub fn validate(&self) -> Result<(), String> {
        if self.project.name.trim().is_empty() {
            return Err("project.name must not be empty".to_string());
        }
        if self.project.version.trim().is_empty() {
            return Err("project.version must not be empty".to_string());
        }
        for (name, dependency) in &self.dependencies {
            dependency.validate(name)?;
        }
        for (name, plugin) in &self.plugins {
            if plugin.version.trim().is_empty() {
                return Err(format!(
                    "plugin `{name}` must declare a version requirement"
                ));
            }
        }
        Ok(())
    }

    /// Canonical YAML serialization (stable key order, no timestamps).
    pub fn to_yaml(&self) -> String {
        serde_yaml::to_string(self).unwrap_or_default()
    }

    /// Minimal manifest used by `marqdo init`.
    pub fn template(project_name: &str) -> Self {
        Self {
            project: ProjectInfo {
                name: project_name.to_string(),
                version: "0.1.0".to_string(),
                marqdo: Some(">=0.1,<1".to_string()),
            },
            dependencies: BTreeMap::new(),
            plugins: BTreeMap::new(),
            capabilities: CapabilityRequests::default(),
        }
    }
}

/// One pinned dependency entry in `marqdo.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Local dependency name.
    pub name: String,
    /// Source description (`path:...` or `git:...@rev`).
    pub source: String,
    /// SHA-256 of the resolved module tree, when computable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// The `marqdo.lock` lockfile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFile {
    /// Lock format version.
    pub lock_version: u32,
    /// Locked packages in deterministic order.
    pub packages: Vec<LockedPackage>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            lock_version: 1,
            packages: Vec::new(),
        }
    }
}

impl LockFile {
    /// Parse a lockfile.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let mut lock: LockFile =
            serde_yaml::from_str(yaml).map_err(|error| format!("invalid {LOCK_FILE}: {error}"))?;
        if lock.lock_version != 1 {
            return Err(format!(
                "unsupported lock_version {} (expected 1)",
                lock.lock_version
            ));
        }
        lock.packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(lock)
    }

    /// Load a lockfile from disk.
    pub fn load(path: &Path) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::from_yaml(&yaml)
    }

    /// Canonical YAML serialization: sorted packages, no absolute paths or
    /// timestamps, so repeated `lock` runs are byte-identical.
    pub fn to_yaml(&self) -> String {
        let mut sorted = self.clone();
        sorted.packages.sort_by(|a, b| a.name.cmp(&b.name));
        serde_yaml::to_string(&sorted).unwrap_or_default()
    }

    /// Find a locked package by name.
    pub fn package(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|package| package.name == name)
    }
}

/// Discover the project root by walking up from `start` to the nearest
/// directory containing `marqdo.yaml`.
pub fn discover_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        if current.join(MANIFEST_FILE).is_file() {
            return Some(current);
        }
        let parent = current.parent()?.to_path_buf();
        if parent == current {
            return None;
        }
        current = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
project:
  name: example
  version: 0.1.0
dependencies:
  geometry:
    path: ./packages/geometry
  quantum:
    git: https://github.com/example/quantum-mq
    rev: 0123456789abcdef0123456789abcdef01234567
capabilities:
  fs:
    read: ["./data/**"]
    write: ["./output/**"]
  net:
    hosts: []
  process: false
"#;

    #[test]
    fn parses_manifest_and_validates_sources() {
        let manifest = Manifest::from_yaml(MANIFEST).unwrap();
        assert_eq!(manifest.project.name, "example");
        assert_eq!(manifest.dependencies.len(), 2);
        let read = &manifest.capabilities.fs.as_ref().unwrap().read;
        assert_eq!(read, &vec!["./data/**".to_string()]);
        assert_eq!(manifest.capabilities.process, Some(false));
    }

    #[test]
    fn git_dependency_requires_rev() {
        let yaml = "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  x:\n    git: https://example/x\n";
        let error = Manifest::from_yaml(yaml).unwrap_err();
        assert!(error.contains("rev"));
    }

    #[test]
    fn lockfile_serialization_is_stable_and_sorted() {
        let lock = LockFile {
            lock_version: 1,
            packages: vec![
                LockedPackage {
                    name: "z".into(),
                    source: "path:z".into(),
                    sha256: None,
                },
                LockedPackage {
                    name: "a".into(),
                    source: "path:a".into(),
                    sha256: Some("abc".into()),
                },
            ],
        };
        let a = lock.to_yaml();
        let b = lock.to_yaml();
        assert_eq!(a, b);
        assert!(a.find("name: a").unwrap() < a.find("name: z").unwrap());
        let parsed = LockFile::from_yaml(&a).unwrap();
        assert_eq!(parsed.packages.len(), 2);
    }

    #[test]
    fn rejects_absolute_dependency_paths() {
        let yaml = "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  x:\n    path: /tmp/x\n";
        assert!(Manifest::from_yaml(yaml).is_err());
    }

    #[test]
    fn discovers_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\n",
        )
        .unwrap();
        assert_eq!(
            discover_root(&nested).unwrap(),
            dir.path()
                .canonicalize()
                .unwrap_or_else(|_| dir.path().to_path_buf())
        );
        assert!(discover_root(Path::new("/")).is_none());
    }
}
