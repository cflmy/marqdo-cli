//! Project manifest, lockfile and dependency resolution.
//!
//! Blueprint §8: `marqdo.yaml` is the project manifest, `marqdo.lock` the
//! reproducible lockfile, and front matter imports are explicit document-level
//! bindings. This crate only parses and serialises these files; it never runs
//! project code and never executes install scripts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Canonical instance id used by a standalone document without a manifest.
pub const DEFAULT_INSTANCE: &str = "root:default";

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<NetRequests>,
    /// May the project request process execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<bool>,
}

impl CapabilityRequests {
    /// Whether the project requests nothing.
    pub fn is_empty(&self) -> bool {
        self.fs.is_none() && self.net.is_none() && self.process.is_none()
    }
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, Dependency>,
    /// Plugin requirements keyed by plugin name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<String, PluginRequirement>,
    /// Capability requests.
    #[serde(default, skip_serializing_if = "CapabilityRequests::is_empty")]
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

/// How a resolved package was obtained. Identity does not depend on this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageOrigin {
    /// The project entry package.
    Root,
    /// A project-relative path dependency.
    Path {
        /// The declared path from `marqdo.yaml` (may be relative).
        declared: PathBuf,
    },
    /// A pinned Git revision (implemented in the environment milestone).
    Git {
        /// Repository URL.
        url: String,
        /// Exact revision.
        rev: String,
    },
}

impl PackageOrigin {
    /// Stable lockfile source string.
    pub fn source(&self) -> String {
        match self {
            PackageOrigin::Root => "root".to_string(),
            PackageOrigin::Path { declared } => format!("path:{}", declared.display()),
            PackageOrigin::Git { url, rev } => format!("git:{url}@{rev}"),
        }
    }
}

/// A dependency resolved from the project manifest.
///
/// `module_root` is a physical location; the `instance_id` is the logical
/// identity used by [`PackageInstance`]/`ModuleId` and by Analysis v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    /// Logical package instance (`root:...` or `dep:...`).
    pub instance_id: String,
    /// Package name (from the dependency manifest or the manifest alias).
    pub name: String,
    /// Package version (from the dependency manifest or `0.0.0`).
    pub version: String,
    /// How the package was obtained.
    pub origin: PackageOrigin,
    /// Exact Git revision, when applicable.
    pub revision: Option<String>,
    /// Content digest of the package tree, when computable.
    pub content_digest: Option<String>,
    /// Physical directory containing the package sources.
    pub module_root: PathBuf,
    /// Entry module relative to `module_root` (`@alias` default).
    pub entry_module: String,
    /// The dependency's own manifest, if present.
    pub manifest: Option<Manifest>,
}

/// A module resolved inside a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    /// Owning package.
    pub package: ResolvedPackage,
    /// Path relative to the package's `module_root`.
    pub relative_path: String,
}

/// Resolve the logical instance id of a root project.
pub fn root_instance(manifest: Option<&Manifest>) -> String {
    match manifest {
        Some(manifest) => format!(
            "root:{}@{}",
            manifest.project.name, manifest.project.version
        ),
        None => DEFAULT_INSTANCE.to_string(),
    }
}

/// Derive a content-addressed dependency instance id.
pub fn dependency_instance_id(name: &str, version: &str, digest: Option<&str>) -> String {
    let suffix = digest
        .filter(|digest| !digest.is_empty())
        .unwrap_or("unlocked");
    let prefix: String = suffix.chars().take(12).collect();
    format!("dep:{name}@{version}#{prefix}")
}

fn normalize_relative(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => {
                parts.push(part.to_string_lossy().to_string());
            }
            _ => return Err(format!("unsafe package-relative path `{}`", path.display())),
        }
    }
    Ok(parts.join("/"))
}

/// Deterministic content digest of a package tree (relative path + bytes).
pub fn hash_tree(root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!(
            "dependency path `{}` does not exist",
            root.display()
        ));
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            files.push((relative, entry.path().to_path_buf()));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Sanitize a logical instance id into one filesystem component.
pub fn env_dir_name(instance: &str) -> String {
    instance
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric()
                || character == '.'
                || character == '-'
                || character == '_'
            {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Resolve entry files and explicit `@dependency` imports.
#[derive(Debug, Clone)]
pub struct PackageResolver {
    root: PathBuf,
    manifest: Option<Manifest>,
    lock: Option<LockFile>,
    cache_root: PathBuf,
    env_root: PathBuf,
    fetch: bool,
}

impl PackageResolver {
    /// Build a resolver for a project root. Fetching is disabled; `check` and
    /// `run` only read path sources, the Git cache and the materialized env.
    pub fn new(
        root: impl Into<PathBuf>,
        manifest: Option<Manifest>,
        lock: Option<LockFile>,
    ) -> Self {
        let root = root.into();
        Self {
            cache_root: root.join(".marqdo/cache"),
            env_root: root.join(".marqdo/env"),
            root,
            manifest,
            lock,
            fetch: false,
        }
    }

    /// Allow `lock`/`sync` to fetch pinned Git revisions (never install
    /// scripts, never submodules).
    pub fn with_fetch(mut self, fetch: bool) -> Self {
        self.fetch = fetch;
        self
    }

    /// Project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project manifest, if the entry is part of a project.
    pub fn manifest(&self) -> Option<&Manifest> {
        self.manifest.as_ref()
    }

    /// Content-addressed cache root.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Materialized environment root.
    pub fn env_root(&self) -> &Path {
        &self.env_root
    }

    /// Resolve the package that owns `entry`.
    pub fn resolve_entry(&self, entry: &Path) -> Result<ResolvedModule, String> {
        let entry = entry
            .canonicalize()
            .map_err(|error| format!("cannot open `{}`: {error}", entry.display()))?;
        let relative = entry.strip_prefix(&self.root).map_err(|_| {
            format!(
                "entry `{}` is outside the project root `{}`",
                entry.display(),
                self.root.display()
            )
        })?;
        Ok(ResolvedModule {
            package: self.root_package()?,
            relative_path: normalize_relative(relative)?,
        })
    }

    fn root_package(&self) -> Result<ResolvedPackage, String> {
        Ok(ResolvedPackage {
            instance_id: root_instance(self.manifest.as_ref()),
            name: self
                .manifest
                .as_ref()
                .map(|manifest| manifest.project.name.clone())
                .unwrap_or_else(|| "default".to_string()),
            version: self
                .manifest
                .as_ref()
                .map(|manifest| manifest.project.version.clone())
                .unwrap_or_else(|| "0.0.0".to_string()),
            origin: PackageOrigin::Root,
            revision: None,
            content_digest: None,
            module_root: self.root.clone(),
            entry_module: "index.mq.md".to_string(),
            manifest: self.manifest.clone(),
        })
    }

    /// Resolve a declared `@alias` dependency from the root project.
    pub fn resolve_alias(&self, alias: &str) -> Result<ResolvedModule, String> {
        self.resolve_alias_from(&self.root_package()?, alias)
    }

    /// Resolve `@alias` from the manifest of the package containing the
    /// importing module. This supports transitive dependencies without
    /// implicit searching.
    pub fn resolve_alias_from(
        &self,
        from: &ResolvedPackage,
        alias: &str,
    ) -> Result<ResolvedModule, String> {
        let manifest = from
            .manifest
            .as_ref()
            .ok_or_else(|| format!("package `{}` has no marqdo.yaml", from.instance_id))?;
        let dependency = manifest
            .dependencies
            .get(alias)
            .ok_or_else(|| format!("unknown dependency `{alias}` in package `{}`", from.name))?;
        self.resolve_dependency(
            &from.module_root,
            manifest,
            alias,
            dependency,
            &from.instance_id,
        )
    }

    fn resolve_dependency(
        &self,
        base: &Path,
        _manifest: &Manifest,
        alias: &str,
        dependency: &Dependency,
        from_instance: &str,
    ) -> Result<ResolvedModule, String> {
        if let Some(path) = &dependency.path {
            let declared_path = base.join(path);
            let canonical = declared_path.canonicalize().map_err(|error| {
                format!(
                    "cannot open dependency `{alias}` at `{}`: {error}",
                    declared_path.display()
                )
            })?;
            let (module_root, entry_module) = package_root_for(&canonical, alias)?;
            let dependency_manifest = load_optional_manifest(&module_root)?;
            let (name, version) = package_name_version(&dependency_manifest, alias);
            let digest = hash_tree(&module_root)?;
            let instance_id = dependency_instance_id(&name, &version, Some(&digest));
            self.check_lock(
                alias,
                &PackageOrigin::Path {
                    declared: path.clone(),
                }
                .source(),
                Some(&digest),
            )?;
            // Prefer a materialized env copy only when it is the same content.
            let env_candidate = self.env_root.join(env_dir_name(&instance_id));
            let module_root = if env_candidate.is_dir()
                && hash_tree(&env_candidate).ok().as_deref() == Some(&digest)
            {
                env_candidate
            } else {
                module_root
            };
            return Ok(ResolvedModule {
                package: ResolvedPackage {
                    instance_id,
                    name,
                    version,
                    origin: PackageOrigin::Path {
                        declared: path.clone(),
                    },
                    revision: None,
                    content_digest: Some(digest),
                    module_root,
                    entry_module: entry_module.clone(),
                    manifest: dependency_manifest,
                },
                relative_path: entry_module,
            });
        }
        let (url, rev) = match (&dependency.git, &dependency.rev) {
            (Some(url), Some(rev)) => (url.clone(), rev.clone()),
            (Some(_), None) => {
                return Err(format!(
                    "dependency `{alias}` uses git without an exact rev"
                ))
            }
            _ => return Err(format!("dependency `{alias}` has no source")),
        };
        let checkout = self.git_checkout(&url, &rev, from_instance)?;
        let dependency_manifest = load_optional_manifest(&checkout)?;
        let (name, version) = package_name_version(&dependency_manifest, alias);
        let digest = hash_tree(&checkout)?;
        let instance_id = dependency_instance_id(&name, &version, Some(&digest));
        // Prefer a materialized env copy when sync produced one.
        let env_candidate = self.env_root.join(env_dir_name(&instance_id));
        let module_root = if env_candidate.is_dir() {
            env_candidate
        } else {
            checkout
        };
        let entry_module = "index.mq.md".to_string();
        if !module_root.join(&entry_module).is_file() {
            return Err(format!(
                "dependency `{alias}` has no {entry_module} at `{}`",
                module_root.display()
            ));
        }
        self.check_lock(
            alias,
            &PackageOrigin::Git {
                url: url.clone(),
                rev: rev.clone(),
            }
            .source(),
            Some(&digest),
        )?;
        Ok(ResolvedModule {
            package: ResolvedPackage {
                instance_id,
                name,
                version,
                origin: PackageOrigin::Git {
                    url,
                    rev: rev.clone(),
                },
                revision: Some(rev),
                content_digest: Some(digest),
                module_root,
                entry_module: entry_module.clone(),
                manifest: dependency_manifest,
            },
            relative_path: entry_module,
        })
    }

    fn check_lock(&self, alias: &str, source: &str, digest: Option<&str>) -> Result<(), String> {
        let Some(lock) = &self.lock else {
            return Ok(());
        };
        let Some(locked) = lock.package(alias) else {
            return Ok(());
        };
        if locked.source != source {
            return Err(format!(
                "dependency `{alias}` source changed from `{}` to `{source}` since marqdo.lock",
                locked.source
            ));
        }
        if let (Some(expected), Some(actual)) = (locked.sha256.as_deref(), digest) {
            if expected != actual {
                return Err(format!(
                    "dependency `{alias}` content changed since marqdo.lock; run `marqdo lock`"
                ));
            }
        }
        Ok(())
    }

    fn git_checkout(&self, url: &str, rev: &str, _from_instance: &str) -> Result<PathBuf, String> {
        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(url.as_bytes());
            hex::encode(hasher.finalize())
        };
        let target =
            self.cache_root
                .join("git")
                .join(format!("{}-{}", &digest[..16], safe_component(rev)));
        if target.is_dir() {
            return Ok(target);
        }
        if !self.fetch {
            return Err(format!(
                "Git dependency `{url}` at `{rev}` is not cached; run `marqdo sync` (or `marqdo lock`) first"
            ));
        }
        let parent = target
            .parent()
            .ok_or_else(|| "invalid Git cache path".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Git cache: {error}"))?;
        let staging = parent.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&staging);
        let result = (|| -> Result<(), String> {
            let status = std::process::Command::new("git")
                .args([
                    "clone",
                    "--quiet",
                    "--no-checkout",
                    "--config",
                    "core.hooksPath=/dev/null",
                    url,
                ])
                .arg(&staging)
                .env("GIT_TERMINAL_PROMPT", "0")
                .status()
                .map_err(|error| format!("cannot execute git: {error}"))?;
            if !status.success() {
                return Err(format!("git clone failed for `{url}`"));
            }
            let status = std::process::Command::new("git")
                .args(["checkout", "--quiet", "--detach", rev])
                .current_dir(&staging)
                .env("GIT_TERMINAL_PROMPT", "0")
                .status()
                .map_err(|error| format!("cannot execute git checkout: {error}"))?;
            if !status.success() {
                return Err(format!(
                    "git checkout of revision `{rev}` failed for `{url}`"
                ));
            }
            let _ = std::fs::remove_dir_all(staging.join(".git"));
            std::fs::rename(&staging, &target)
                .map_err(|error| format!("cannot publish Git cache: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result.map(|()| target)
    }

    /// Materialize the locked path/Git packages into `.marqdo/env`.
    pub fn materialize_env(&self, manifest: &Manifest, lock: &LockFile) -> Result<(), String> {
        let fetch_resolver = PackageResolver {
            fetch: true,
            lock: Some(lock.clone()),
            ..self.clone()
        };
        let mut packages = Vec::new();
        for name in manifest.dependencies.keys() {
            let resolved = fetch_resolver.resolve_alias(name)?;
            packages.push((name.clone(), resolved.package));
        }
        let staging = self.env_root.with_file_name(format!(
            "env.tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)
            .map_err(|error| format!("cannot create environment staging: {error}"))?;
        let result = (|| -> Result<(), String> {
            let mut records = Vec::new();
            for (alias, package) in &packages {
                let directory = staging.join(env_dir_name(&package.instance_id));
                copy_tree(&package.module_root, &directory)?;
                records.push(serde_json::json!({
                    "alias": alias,
                    "instance_id": package.instance_id,
                    "source": package.origin.source(),
                    "sha256": package.content_digest,
                }));
            }
            let index = serde_json::json!({
                "schema": "marqdo.env",
                "schema_version": 1,
                "packages": records,
            });
            std::fs::write(
                staging.join("env.json"),
                serde_json::to_vec_pretty(&index).unwrap_or_default(),
            )
            .map_err(|error| format!("cannot write env index: {error}"))?;
            let backup = self.env_root.with_file_name(format!(
                "env.old-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_nanos())
                    .unwrap_or(0)
            ));
            if self.env_root.exists() {
                std::fs::rename(&self.env_root, &backup)
                    .map_err(|error| format!("cannot replace old environment: {error}"))?;
            }
            if let Err(error) = std::fs::rename(&staging, &self.env_root) {
                if backup.exists() {
                    let _ = std::fs::rename(&backup, &self.env_root);
                }
                return Err(format!("cannot publish environment: {error}"));
            }
            let _ = std::fs::remove_dir_all(&backup);
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
    }

    /// Resolve a path relative to an already resolved package, rejecting
    /// traversal outside that package's module root.
    pub fn resolve_relative(
        &self,
        package: &ResolvedPackage,
        importing: &Path,
        target: &str,
    ) -> Result<ResolvedModule, String> {
        let base = importing
            .parent()
            .ok_or_else(|| "importing module has no parent directory".to_string())?;
        let candidate = base.join(target);
        let canonical = candidate
            .canonicalize()
            .map_err(|error| format!("cannot open `{}`: {error}", candidate.display()))?;
        let relative = canonical.strip_prefix(&package.module_root).map_err(|_| {
            format!(
                "import `{target}` escapes package `{}`",
                package.instance_id
            )
        })?;
        Ok(ResolvedModule {
            package: package.clone(),
            relative_path: normalize_relative(relative)?,
        })
    }
}

fn package_root_for(canonical: &Path, alias: &str) -> Result<(PathBuf, String), String> {
    if canonical.is_dir() {
        let entry = canonical.join("index.mq.md");
        if !entry.is_file() {
            return Err(format!(
                "dependency `{alias}` directory `{}` has no index.mq.md",
                canonical.display()
            ));
        }
        Ok((canonical.to_path_buf(), "index.mq.md".to_string()))
    } else {
        let parent = canonical
            .parent()
            .ok_or_else(|| format!("dependency `{alias}` has no parent directory"))?
            .to_path_buf();
        let name = canonical
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| format!("dependency `{alias}` has no file name"))?;
        Ok((parent, name))
    }
}

fn load_optional_manifest(root: &Path) -> Result<Option<Manifest>, String> {
    if root.join(MANIFEST_FILE).is_file() {
        Ok(Some(Manifest::load(&root.join(MANIFEST_FILE))?))
    } else {
        Ok(None)
    }
}

fn package_name_version(manifest: &Option<Manifest>, alias: &str) -> (String, String) {
    (
        manifest
            .as_ref()
            .map(|manifest| manifest.project.name.clone())
            .unwrap_or_else(|| alias.to_string()),
        manifest
            .as_ref()
            .map(|manifest| manifest.project.version.clone())
            .unwrap_or_else(|| "0.0.0".to_string()),
    )
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect()
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!(
            "package source `{}` is not a directory",
            source.display()
        ));
    }
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| format!("cannot walk package: {error}"))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| "package path escaped its root".to_string())?;
        if relative.as_os_str() == ".git" || relative.starts_with(".git/") {
            continue;
        }
        let destination = target.join(relative);
        if entry.file_type().is_symlink() {
            return Err(format!(
                "package contains a symlink `{}`; refusing to materialize it",
                entry.path().display()
            ));
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination)
                .map_err(|error| format!("cannot create `{}`: {error}", destination.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
            }
            std::fs::copy(entry.path(), &destination).map_err(|error| {
                format!(
                    "cannot copy `{}` to `{}`: {error}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

/// One pinned dependency entry in `marqdo.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Local dependency name.
    pub name: String,
    /// Logical package instance id (lock format v2).
    #[serde(default)]
    pub instance_id: String,
    /// Source description (`path:...` or `git:...@rev`).
    pub source: String,
    /// SHA-256 of the resolved package tree, when computable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl LockedPackage {
    /// Fill a v1 entry's missing instance id deterministically.
    fn migrate_instance_id(&mut self) {
        if self.instance_id.is_empty() {
            let digest = self.sha256.as_deref().unwrap_or("unlocked");
            let prefix: String = digest.chars().take(12).collect();
            self.instance_id = format!("dep:{}@0.0.0#{prefix}", self.name);
        }
    }
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
            lock_version: 2,
            packages: Vec::new(),
        }
    }
}

impl LockFile {
    /// Parse a lockfile.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let mut lock: LockFile =
            serde_yaml::from_str(yaml).map_err(|error| format!("invalid {LOCK_FILE}: {error}"))?;
        if lock.lock_version != 1 && lock.lock_version != 2 {
            return Err(format!(
                "unsupported lock_version {} (expected 1 or 2)",
                lock.lock_version
            ));
        }
        for package in &mut lock.packages {
            package.migrate_instance_id();
        }
        lock.lock_version = 2;
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
            lock_version: 2,
            packages: vec![
                LockedPackage {
                    name: "z".into(),
                    instance_id: "dep:z@0.0.0#unlocked".into(),
                    source: "path:z".into(),
                    sha256: None,
                },
                LockedPackage {
                    name: "a".into(),
                    instance_id: "dep:a@0.0.0#abc".into(),
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
        assert_eq!(parsed.lock_version, 2);
        assert_eq!(parsed.packages.len(), 2);
    }

    #[test]
    fn v1_lock_migrates_to_logical_instance_ids() {
        let v1 = "lock_version: 1\npackages:\n- name: geometry\n  source: path:./geometry\n  sha256: abcdef0123456789\n";
        let lock = LockFile::from_yaml(v1).unwrap();
        assert_eq!(lock.lock_version, 2);
        assert_eq!(
            lock.packages[0].instance_id,
            "dep:geometry@0.0.0#abcdef012345"
        );
    }

    #[test]
    fn package_resolution_is_stable_across_path_moves() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  geometry:\n    path: ./deps/geometry\n",
        )
        .unwrap();
        let first = root.join("deps/geometry");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::write(
            first.join(MANIFEST_FILE),
            "project:\n  name: geometry\n  version: 0.1.0\n",
        )
        .unwrap();
        std::fs::write(first.join("index.mq.md"), "## f\n\n*1*\n").unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver = PackageResolver::new(root.clone(), Some(manifest.clone()), None);
        let before = resolver
            .resolve_alias("geometry")
            .unwrap()
            .package
            .instance_id;

        let second = root.join("moved/geometry");
        std::fs::create_dir_all(second.parent().unwrap()).unwrap();
        std::fs::rename(&first, &second).unwrap();
        let mut manifest = manifest;
        manifest.dependencies.get_mut("geometry").unwrap().path = Some("./moved/geometry".into());
        std::fs::write(root.join(MANIFEST_FILE), manifest.to_yaml()).unwrap();
        let resolver = PackageResolver::new(root, Some(manifest), None);
        let after = resolver
            .resolve_alias("geometry")
            .unwrap()
            .package
            .instance_id;
        assert_eq!(before, after);
        assert!(after.starts_with("dep:geometry@0.1.0#"));
    }

    #[test]
    fn rejects_absolute_dependency_paths() {
        let yaml = "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  x:\n    path: /tmp/x\n";
        assert!(Manifest::from_yaml(yaml).is_err());
    }

    fn have_git() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn pinned_git_dependency_resolves_and_reuses_cache() {
        if !have_git() {
            eprintln!("skipping: git is unavailable");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let dep = dir.path().join("dep");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&dep).unwrap();
        std::fs::write(
            dep.join(MANIFEST_FILE),
            "project:\n  name: remote\n  version: 0.2.0\n",
        )
        .unwrap();
        std::fs::write(dep.join("index.mq.md"), "## hello\n\n*1*\n").unwrap();
        git(&dep, &["init", "-q"]);
        git(&dep, &["add", "."]);
        git(&dep, &["commit", "-q", "-m", "initial"]);
        let rev = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&dep)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            format!(
                "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  remote:\n    git: {}\n    rev: {}\n",
                dep.display(),
                rev.trim()
            ),
        )
        .unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver =
            PackageResolver::new(root.clone(), Some(manifest.clone()), None).with_fetch(true);
        let resolved = resolver.resolve_alias("remote").unwrap();
        assert_eq!(resolved.package.name, "remote");
        assert_eq!(resolved.package.version, "0.2.0");
        assert!(resolved
            .package
            .instance_id
            .starts_with("dep:remote@0.2.0#"));
        assert_eq!(resolved.package.revision.as_deref(), Some(rev.trim()));
        assert!(resolved.package.module_root.join("index.mq.md").is_file());

        // A later resolver without fetch reads the content-addressed cache.
        let offline = PackageResolver::new(root, Some(manifest), None);
        let again = offline.resolve_alias("remote").unwrap();
        assert_eq!(again.package.instance_id, resolved.package.instance_id);
    }

    #[test]
    fn sync_materializes_a_content_checked_environment() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let dep = root.join("deps/geometry");
        std::fs::create_dir_all(&dep).unwrap();
        std::fs::write(
            dep.join(MANIFEST_FILE),
            "project:\n  name: geometry\n  version: 0.1.0\n",
        )
        .unwrap();
        std::fs::write(dep.join("index.mq.md"), "## area\n\n*1*\n").unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  geometry:\n    path: ./deps/geometry\n",
        )
        .unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver =
            PackageResolver::new(root.clone(), Some(manifest.clone()), None).with_fetch(true);
        let resolved = resolver.resolve_alias("geometry").unwrap();
        let lock = LockFile {
            lock_version: 2,
            packages: vec![LockedPackage {
                name: "geometry".into(),
                instance_id: resolved.package.instance_id.clone(),
                source: resolved.package.origin.source(),
                sha256: resolved.package.content_digest.clone(),
            }],
        };
        resolver.materialize_env(&manifest, &lock).unwrap();
        let env = root.join(".marqdo/env");
        assert!(env.join("env.json").is_file());
        assert!(env
            .join(env_dir_name(&resolved.package.instance_id))
            .join("index.mq.md")
            .is_file());
        // A resolver with the lock reads the materialized copy.
        let read = PackageResolver::new(root, Some(manifest), Some(lock));
        let again = read.resolve_alias("geometry").unwrap();
        assert_eq!(again.package.instance_id, resolved.package.instance_id);
        assert!(again.package.module_root.starts_with(env));
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
