//! Project manifest, lockfile and dependency resolution.
//!
//! Blueprint §8: `marqdo.yaml` is the project manifest, `marqdo.lock` the
//! reproducible lockfile, and front matter imports are explicit document-level
//! bindings. This crate only parses and serialises these files; it never runs
//! project code and never executes install scripts.

mod fileset;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use fileset::CONTENT_PROFILE_VERSION;

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
            (None, Some(_)) if !is_full_commit(self.rev.as_deref().unwrap_or("")) => Err(format!(
                "dependency `{name}` git rev must be a full lowercase 40- or 64-character commit hash"
            )),
            // `has_root`, not `is_absolute`: on Windows a drive-less rooted
            // path such as `/tmp/x` or `\tmp\x` is not `is_absolute()` but
            // still escapes the project when joined. Reject every rooted path.
            (Some(path), None) if path.has_root() => Err(format!(
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
///
/// The full content digest is part of the identity; a short prefix is not
/// collision-resistant enough to distinguish package instances (ADR-0016).
pub fn dependency_instance_id(name: &str, version: &str, digest: &str) -> String {
    format!("dep:{name}@{version}#{digest}")
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
///
/// Selection and encoding are defined by `content_profile_version` 1 in
/// [`fileset`]; hashing and materialization share that selection.
pub fn hash_tree(root: &Path) -> Result<String, String> {
    fileset::hash_tree(root)
}

/// Derive an injective filesystem component for a package instance.
pub fn env_dir_name(instance: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(instance.as_bytes());
    format!("pkg-{}", hex::encode(hasher.finalize()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitCacheMarker {
    schema_version: u32,
    content_profile_version: u32,
    url: String,
    rev: String,
    content_digest: String,
}

/// Maximum number of package instances in one lock graph.
pub const MAX_GRAPH_PACKAGES: usize = 4096;
/// Maximum dependency depth in one lock graph.
pub const MAX_GRAPH_DEPTH: usize = 128;

/// A resolved lock graph with each instance's physical source location.
#[derive(Debug, Clone)]
pub struct ResolvedLockGraph {
    /// Reproducible lock document.
    pub lock: LockFile,
    /// Resolved packages keyed by instance id (including the root).
    pub packages: BTreeMap<String, ResolvedPackage>,
}

#[derive(Debug, Default)]
struct GraphState {
    packages: BTreeMap<String, ResolvedPackage>,
    edges: BTreeMap<String, BTreeMap<String, String>>,
    visiting: BTreeSet<String>,
    done: BTreeSet<String>,
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
    /// Only explicit lock generation may resolve packages without a lock.
    allow_unlocked: bool,
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
            allow_unlocked: false,
        }
    }

    /// Allow `lock`/`sync` to fetch pinned Git revisions (never install
    /// scripts, never submodules).
    pub fn with_fetch(mut self, fetch: bool) -> Self {
        self.fetch = fetch;
        self
    }

    /// Allow resolution without a lock. Intended for `marqdo lock` only;
    /// normal run/check resolution is fail-closed.
    pub fn with_unlocked(mut self, allow_unlocked: bool) -> Self {
        self.allow_unlocked = allow_unlocked;
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
            let instance_id = dependency_instance_id(&name, &version, &digest);
            let origin = PackageOrigin::Path {
                declared: path.clone(),
            };
            let locked_source = self.locked_source(&origin, &module_root)?;
            self.check_locked_edge(from_instance, alias, &locked_source, &digest)?;
            // Prefer a materialized env copy only when it is the same content.
            // Lock generation never reads env; it must hash source truth.
            let env_candidate = self.env_root.join(env_dir_name(&instance_id));
            let module_root = if self.lock.is_some()
                && env_candidate.is_dir()
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
        let instance_id = dependency_instance_id(&name, &version, &digest);
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

    fn locked_source(
        &self,
        origin: &PackageOrigin,
        module_root: &Path,
    ) -> Result<LockedSource, String> {
        match origin {
            PackageOrigin::Root => Err("the root package is not a lock source".to_string()),
            PackageOrigin::Git { url, rev } => Ok(LockedSource::Git {
                url: url.clone(),
                rev: rev.clone(),
            }),
            PackageOrigin::Path { .. } => {
                let root = self
                    .root
                    .canonicalize()
                    .map_err(|error| format!("cannot canonicalize project root: {error}"))?;
                let module = module_root.canonicalize().map_err(|error| {
                    format!(
                        "cannot canonicalize package `{}`: {error}",
                        module_root.display()
                    )
                })?;
                let relative = module.strip_prefix(&root).map_err(|_| {
                    format!(
                        "path dependency `{}` escapes the project root; use a pinned Git source or move it inside the project",
                        module.display()
                    )
                })?;
                Ok(LockedSource::Path {
                    path: normalize_relative(relative)?,
                })
            }
        }
    }

    /// Fail-closed check of one locked `(parent instance, alias)` edge.
    fn check_locked_edge(
        &self,
        from_instance: &str,
        alias: &str,
        source: &LockedSource,
        digest: &str,
    ) -> Result<(), String> {
        let Some(lock) = &self.lock else {
            if self.allow_unlocked {
                return Ok(());
            }
            return Err(
                "marqdo.lock is missing; run `marqdo lock` before checking or running this project"
                    .to_string(),
            );
        };
        if from_instance.starts_with("root:") && lock.root.package != from_instance {
            return Err(format!(
                "root manifest changed since marqdo.lock (resolved `{from_instance}`, lock has `{}`); run `marqdo lock`",
                lock.root.package
            ));
        }
        let Some(target) = lock.edge(from_instance, alias) else {
            return Err(format!(
                "dependency `{alias}` of `{from_instance}` is not recorded in marqdo.lock; run `marqdo lock`"
            ));
        };
        let Some(record) = lock.package(target) else {
            return Err(format!(
                "marqdo.lock edge `{from_instance}` -> `{alias}` points at missing package `{target}`; run `marqdo lock`"
            ));
        };
        if record.content_digest != digest {
            return Err(
                "lockfile is out of date or dependency contents changed; run `marqdo lock`"
                    .to_string(),
            );
        }
        if &record.source != source {
            return Err(format!(
                "dependency `{alias}` source changed since marqdo.lock; run `marqdo lock`"
            ));
        }
        Ok(())
    }

    fn git_checkout(&self, url: &str, rev: &str, _from_instance: &str) -> Result<PathBuf, String> {
        if !is_full_commit(rev) {
            return Err(format!(
                "Git dependency `{url}` must pin a full lowercase 40- or 64-character commit hash, got `{rev}`"
            ));
        }
        let mut url_hasher = Sha256::new();
        url_hasher.update(url.as_bytes());
        let url_digest = hex::encode(url_hasher.finalize());
        let git_root = self.cache_root.join("git");
        let target = git_root.join(format!("{url_digest}-{rev}"));
        let marker_path = git_root.join(format!("{url_digest}-{rev}.meta.json"));

        if target.is_dir() {
            match self.verify_git_cache(&target, &marker_path, url, rev) {
                Ok(()) => return Ok(target),
                Err(error) if !self.fetch => {
                    return Err(format!(
                        "Git cache for `{url}` at `{rev}` failed verification ({error}); run `marqdo sync` with fetch enabled to repair it"
                    ))
                }
                Err(_) => {
                    let _ = std::fs::remove_dir_all(&target);
                    let _ = std::fs::remove_file(&marker_path);
                }
            }
        } else if !self.fetch {
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
            let head = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&staging)
                .output()
                .map_err(|error| format!("cannot execute git rev-parse: {error}"))?;
            if !head.status.success() {
                return Err(format!("cannot read checked-out HEAD for `{url}`"));
            }
            let actual = String::from_utf8_lossy(&head.stdout).trim().to_string();
            if actual != rev {
                return Err(format!(
                    "git checkout for `{url}` produced commit `{actual}`, expected `{rev}`"
                ));
            }
            let _ = std::fs::remove_dir_all(staging.join(".git"));
            let content_digest = hash_tree(&staging)?;
            std::fs::rename(&staging, &target)
                .map_err(|error| format!("cannot publish Git cache: {error}"))?;
            let marker = GitCacheMarker {
                schema_version: 1,
                content_profile_version: CONTENT_PROFILE_VERSION,
                url: url.to_string(),
                rev: rev.to_string(),
                content_digest,
            };
            std::fs::write(
                &marker_path,
                serde_json::to_vec_pretty(&marker).unwrap_or_default(),
            )
            .map_err(|error| format!("cannot write Git cache marker: {error}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            let _ = std::fs::remove_dir_all(&target);
            let _ = std::fs::remove_file(&marker_path);
        }
        result.map(|()| target)
    }

    fn verify_git_cache(
        &self,
        target: &Path,
        marker_path: &Path,
        url: &str,
        rev: &str,
    ) -> Result<(), String> {
        let bytes = std::fs::read(marker_path)
            .map_err(|error| format!("missing Git cache marker: {error}"))?;
        let marker: GitCacheMarker = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Git cache marker: {error}"))?;
        if marker.schema_version != 1 {
            return Err(format!(
                "unsupported Git cache marker schema {}",
                marker.schema_version
            ));
        }
        if marker.content_profile_version != CONTENT_PROFILE_VERSION {
            return Err(format!(
                "Git cache was built with content profile {}, this build uses {}",
                marker.content_profile_version, CONTENT_PROFILE_VERSION
            ));
        }
        if marker.url != url || marker.rev != rev {
            return Err("Git cache marker does not match the requested source".to_string());
        }
        let actual = hash_tree(target)?;
        if actual != marker.content_digest {
            return Err("Git cache content does not match its recorded digest".to_string());
        }
        Ok(())
    }

    /// Build the complete lock graph for this project.
    ///
    /// The resolver must be in explicit lock-generation mode
    /// ([`PackageResolver::with_unlocked`]); this never writes anything.
    pub fn resolve_lock_graph(&self) -> Result<ResolvedLockGraph, String> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| "cannot lock a project without marqdo.yaml".to_string())?;
        let root = self.root_package()?;
        let mut state = GraphState::default();
        self.visit_graph(&root, &mut state, 0)?;
        let root_dependencies = state.edges.remove(&root.instance_id).unwrap_or_default();
        let mut packages = Vec::new();
        for (instance_id, package) in &state.packages {
            if instance_id == &root.instance_id {
                continue;
            }
            let content_digest = package
                .content_digest
                .clone()
                .ok_or_else(|| format!("package `{instance_id}` has no content digest"))?;
            packages.push(LockedPackage {
                instance_id: instance_id.clone(),
                name: package.name.clone(),
                version: package.version.clone(),
                source: self.locked_source(&package.origin, &package.module_root)?,
                content_digest,
                manifest_digest: package.manifest.as_ref().map(manifest_digest),
                dependencies: state.edges.remove(instance_id).unwrap_or_default(),
            });
        }
        packages.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        let lock = LockFile {
            lock_version: LOCK_VERSION,
            content_profile_version: CONTENT_PROFILE_VERSION,
            root: LockedRoot {
                package: root.instance_id.clone(),
                manifest_digest: manifest_digest(manifest),
            },
            packages,
            root_dependencies,
        };
        lock.validate()?;
        Ok(ResolvedLockGraph {
            lock,
            packages: state.packages,
        })
    }

    fn visit_graph(
        &self,
        package: &ResolvedPackage,
        state: &mut GraphState,
        depth: usize,
    ) -> Result<(), String> {
        let instance = package.instance_id.clone();
        if state.done.contains(&instance) {
            return Ok(());
        }
        if state.visiting.contains(&instance) {
            return Err(format!(
                "dependency cycle detected at package `{instance}`; dependency graphs must be acyclic"
            ));
        }
        if depth > MAX_GRAPH_DEPTH {
            return Err(format!(
                "dependency graph exceeds the maximum depth of {MAX_GRAPH_DEPTH}"
            ));
        }
        if state.packages.len() >= MAX_GRAPH_PACKAGES {
            return Err(format!(
                "dependency graph exceeds the maximum of {MAX_GRAPH_PACKAGES} packages"
            ));
        }
        state.visiting.insert(instance.clone());
        state.packages.insert(instance.clone(), package.clone());
        let mut edges = BTreeMap::new();
        if let Some(manifest) = &package.manifest {
            for alias in manifest.dependencies.keys() {
                let resolved = self.resolve_alias_from(package, alias)?;
                edges.insert(alias.clone(), resolved.package.instance_id.clone());
                self.visit_graph(&resolved.package, state, depth + 1)?;
            }
        }
        state.edges.insert(instance.clone(), edges);
        state.visiting.remove(&instance);
        state.done.insert(instance);
        Ok(())
    }

    fn resolve_locked_package(&self, locked: &LockedPackage) -> Result<ResolvedPackage, String> {
        let (module_root, origin, revision) = match &locked.source {
            LockedSource::Path { path } => {
                let root = self
                    .root
                    .canonicalize()
                    .map_err(|error| format!("cannot canonicalize project root: {error}"))?;
                let candidate = self
                    .root
                    .join(path)
                    .canonicalize()
                    .map_err(|error| format!("cannot open locked package `{path}`: {error}"))?;
                if !candidate.starts_with(&root) {
                    return Err(format!(
                        "locked package path `{path}` escapes the project root"
                    ));
                }
                if !candidate.is_dir() {
                    return Err(format!("locked package path `{path}` is not a directory"));
                }
                (
                    candidate,
                    PackageOrigin::Path {
                        declared: PathBuf::from(path),
                    },
                    None,
                )
            }
            LockedSource::Git { url, rev } => (
                self.git_checkout(url, rev, &self.root_package()?.instance_id)?,
                PackageOrigin::Git {
                    url: url.clone(),
                    rev: rev.clone(),
                },
                Some(rev.clone()),
            ),
        };
        let digest = hash_tree(&module_root)?;
        if digest != locked.content_digest {
            return Err(
                "lockfile is out of date or dependency contents changed; run `marqdo lock`"
                    .to_string(),
            );
        }
        let entry_module = "index.mq.md".to_string();
        if !module_root.join(&entry_module).is_file() {
            return Err(format!(
                "locked package `{}` has no {entry_module} at `{}`",
                locked.instance_id,
                module_root.display()
            ));
        }
        Ok(ResolvedPackage {
            instance_id: locked.instance_id.clone(),
            name: locked.name.clone(),
            version: locked.version.clone(),
            origin,
            revision,
            content_digest: Some(locked.content_digest.clone()),
            module_root,
            entry_module,
            manifest: None,
        })
    }

    /// Materialize every package in the locked closure into `.marqdo/env`.
    ///
    /// The environment is staged and published atomically; each package is
    /// copied with the exact file-set digest recorded in the lock.
    pub fn materialize_env(&self, lock: &LockFile) -> Result<(), String> {
        lock.validate()?;
        let mut packages = Vec::new();
        for locked in &lock.packages {
            packages.push(self.resolve_locked_package(locked)?);
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
            for package in &packages {
                let directory = staging.join(env_dir_name(&package.instance_id));
                let expected = package.content_digest.as_deref().ok_or_else(|| {
                    format!(
                        "package `{}` has no content digest; refusing to materialize",
                        package.instance_id
                    )
                })?;
                fileset::materialize_file_set(&package.module_root, &directory, expected)?;
                records.push(serde_json::json!({
                    "instance_id": package.instance_id,
                    "dir": env_dir_name(&package.instance_id),
                    "source": package.origin.source(),
                    "content_digest": expected,
                }));
            }
            let index = serde_json::json!({
                "schema": "marqdo.env",
                "schema_version": 2,
                "content_profile_version": CONTENT_PROFILE_VERSION,
                "lock_digest": lock.digest(),
                "root_dependencies": lock.root_dependencies,
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

/// Lock format version written by this build.
pub const LOCK_VERSION: u32 = 3;

/// One package instance in `marqdo.lock` v3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Unique package instance id (`dep:<name>@<version>#<digest>`).
    pub instance_id: String,
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Where the package came from.
    pub source: LockedSource,
    /// SHA-256 of the package file set (64 lowercase hex).
    pub content_digest: String,
    /// SHA-256 of the package manifest, when the package has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
    /// Alias -> target package instance for this package's dependencies.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// A structured package source in lock v3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockedSource {
    /// Project-relative directory containing the package.
    Path {
        /// Normalized `/`-separated path relative to the project root.
        path: String,
    },
    /// Pinned Git revision.
    Git {
        /// Repository URL.
        url: String,
        /// Full lowercase commit hash.
        rev: String,
    },
}

impl LockedSource {
    /// Legacy lock source string used by environment metadata.
    pub fn source_string(&self) -> String {
        match self {
            LockedSource::Path { path } => format!("path:{path}"),
            LockedSource::Git { url, rev } => format!("git:{url}@{rev}"),
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            LockedSource::Path { path } => {
                if path.is_empty() || path.contains('\\') {
                    return Err(format!("invalid locked path source `{path}`"));
                }
                let normalized = normalize_relative(Path::new(path))
                    .map_err(|error| format!("invalid locked path source `{path}`: {error}"))?;
                if normalized != *path {
                    return Err(format!(
                        "locked path source `{path}` is not normalized (expected `{normalized}`)"
                    ));
                }
            }
            LockedSource::Git { url, rev } => {
                if url.trim().is_empty() {
                    return Err("locked git source has an empty URL".to_string());
                }
                if !is_full_commit(rev) {
                    return Err(format!(
                        "locked git revision `{rev}` must be a full lowercase 40- or 64-character commit hash"
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Root entry of a lock v3 graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedRoot {
    /// Logical root package instance (`root:<name>@<version>`).
    pub package: String,
    /// SHA-256 of the canonical root manifest.
    pub manifest_digest: String,
}

impl Default for LockedRoot {
    fn default() -> Self {
        Self {
            package: DEFAULT_INSTANCE.to_string(),
            manifest_digest: String::new(),
        }
    }
}

/// The `marqdo.lock` package-instance graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFile {
    /// Lock format version (3).
    pub lock_version: u32,
    /// Package file-set profile used for every digest.
    pub content_profile_version: u32,
    /// Root package identity and manifest digest.
    pub root: LockedRoot,
    /// Package instances in deterministic (id-sorted) order.
    #[serde(default)]
    pub packages: Vec<LockedPackage>,
    /// Root alias -> target package instance.
    #[serde(default)]
    pub root_dependencies: BTreeMap<String, String>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            lock_version: LOCK_VERSION,
            content_profile_version: CONTENT_PROFILE_VERSION,
            root: LockedRoot::default(),
            packages: Vec::new(),
            root_dependencies: BTreeMap::new(),
        }
    }
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_full_commit(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Canonical digest of a manifest used by the lock root entry.
pub fn manifest_digest(manifest: &Manifest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"marqdo.manifest.v1\0");
    hasher.update(manifest.to_yaml().as_bytes());
    hex::encode(hasher.finalize())
}

impl LockFile {
    /// Parse and validate a lockfile. Only lock v3 is accepted.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct VersionProbe {
            lock_version: u32,
        }
        let probe: VersionProbe =
            serde_yaml::from_str(yaml).map_err(|error| format!("invalid {LOCK_FILE}: {error}"))?;
        if probe.lock_version != LOCK_VERSION {
            return Err(format!(
                "unsupported lock_version {}; lock v3 is required. Run `marqdo lock` to regenerate the graph",
                probe.lock_version
            ));
        }
        let mut lock: LockFile =
            serde_yaml::from_str(yaml).map_err(|error| format!("invalid {LOCK_FILE}: {error}"))?;
        lock.packages
            .sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        lock.validate()?;
        Ok(lock)
    }

    /// Load a lockfile from disk.
    pub fn load(path: &Path) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::from_yaml(&yaml)
    }

    /// Canonical YAML serialization: sorted by instance id, no timestamps.
    pub fn to_yaml(&self) -> String {
        let mut sorted = self.clone();
        sorted
            .packages
            .sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        serde_yaml::to_string(&sorted).unwrap_or_default()
    }

    /// Digest of the canonical lock document, recorded in `env.json`.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.to_yaml().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Find a locked package by instance id.
    pub fn package(&self, instance_id: &str) -> Option<&LockedPackage> {
        self.packages
            .iter()
            .find(|package| package.instance_id == instance_id)
    }

    /// Resolve a dependency edge by `(parent instance, alias)`.
    pub fn edge(&self, from_instance: &str, alias: &str) -> Option<&str> {
        if from_instance == self.root.package {
            return self.root_dependencies.get(alias).map(String::as_str);
        }
        self.package(from_instance)
            .and_then(|package| package.dependencies.get(alias))
            .map(String::as_str)
    }

    /// Validate graph invariants. Fail-closed: every problem is an error.
    pub fn validate(&self) -> Result<(), String> {
        if self.lock_version != LOCK_VERSION {
            return Err(format!(
                "unsupported lock_version {}; expected {LOCK_VERSION}",
                self.lock_version
            ));
        }
        if self.content_profile_version != CONTENT_PROFILE_VERSION {
            return Err(format!(
                "lock was written with content profile {}; this build supports {CONTENT_PROFILE_VERSION}. Run `marqdo lock`",
                self.content_profile_version
            ));
        }
        if !self.root.package.starts_with("root:") {
            return Err(format!(
                "lock root package `{}` is not a root instance",
                self.root.package
            ));
        }
        if !is_hex_digest(&self.root.manifest_digest) {
            return Err(
                "lock root manifest_digest is not a 64-character lowercase hex digest".to_string(),
            );
        }

        let mut ids = BTreeSet::new();
        for package in &self.packages {
            if !ids.insert(package.instance_id.as_str()) {
                return Err(format!(
                    "duplicate package instance `{}` in {LOCK_FILE}",
                    package.instance_id
                ));
            }
            if !is_hex_digest(&package.content_digest) {
                return Err(format!(
                    "package `{}` content_digest is not a 64-character lowercase hex digest",
                    package.instance_id
                ));
            }
            if let Some(digest) = &package.manifest_digest {
                if !is_hex_digest(digest) {
                    return Err(format!(
                        "package `{}` manifest_digest is not a 64-character lowercase hex digest",
                        package.instance_id
                    ));
                }
            }
            let expected =
                dependency_instance_id(&package.name, &package.version, &package.content_digest);
            if expected != package.instance_id {
                return Err(format!(
                    "package id `{}` does not match its name/version/content digest (`{expected}`)",
                    package.instance_id
                ));
            }
            package.source.validate()?;
            for (alias, target) in &package.dependencies {
                if alias.is_empty() {
                    return Err(format!(
                        "package `{}` has an empty dependency alias",
                        package.instance_id
                    ));
                }
                if target == &package.instance_id {
                    return Err(format!(
                        "package `{}` depends on itself via `{alias}`",
                        package.instance_id
                    ));
                }
            }
        }

        for (alias, target) in &self.root_dependencies {
            if alias.is_empty() {
                return Err("lock root_dependencies contains an empty alias".to_string());
            }
            if !ids.contains(target.as_str()) {
                return Err(format!(
                    "root dependency `{alias}` points at missing package `{target}`"
                ));
            }
        }
        for package in &self.packages {
            for (alias, target) in &package.dependencies {
                if !ids.contains(target.as_str()) {
                    return Err(format!(
                        "dependency `{alias}` of `{}` points at missing package `{target}`",
                        package.instance_id
                    ));
                }
            }
        }

        // Every recorded package must be reachable from the root graph.
        let mut reachable = BTreeSet::new();
        let mut queue: Vec<&str> = self
            .root_dependencies
            .values()
            .map(String::as_str)
            .collect();
        while let Some(id) = queue.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(package) = self.package(id) {
                for target in package.dependencies.values() {
                    queue.push(target);
                }
            }
        }
        for package in &self.packages {
            if !reachable.contains(package.instance_id.as_str()) {
                return Err(format!(
                    "package `{}` is not reachable from root_dependencies; regenerate the lock",
                    package.instance_id
                ));
            }
        }
        Ok(())
    }

    /// Validate that this lock belongs to the given root manifest.
    pub fn validate_root(&self, manifest: &Manifest) -> Result<(), String> {
        let expected = root_instance(Some(manifest));
        if self.root.package != expected {
            return Err(format!(
                "root manifest changed since marqdo.lock (expected `{expected}`, lock has `{}`); run `marqdo lock`",
                self.root.package
            ));
        }
        if self.root.manifest_digest != manifest_digest(manifest) {
            return Err("root manifest changed since marqdo.lock; run `marqdo lock`".to_string());
        }
        Ok(())
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
        let digest = "a".repeat(64);
        let other = "b".repeat(64);
        let lock = LockFile {
            lock_version: LOCK_VERSION,
            content_profile_version: CONTENT_PROFILE_VERSION,
            root: LockedRoot {
                package: "root:p@0.1.0".into(),
                manifest_digest: digest.clone(),
            },
            packages: vec![
                LockedPackage {
                    instance_id: dependency_instance_id("z", "0.0.0", &digest),
                    name: "z".into(),
                    version: "0.0.0".into(),
                    source: LockedSource::Path { path: "z".into() },
                    content_digest: digest.clone(),
                    manifest_digest: None,
                    dependencies: BTreeMap::new(),
                },
                LockedPackage {
                    instance_id: dependency_instance_id("a", "0.0.0", &other),
                    name: "a".into(),
                    version: "0.0.0".into(),
                    source: LockedSource::Path { path: "a".into() },
                    content_digest: other.clone(),
                    manifest_digest: None,
                    dependencies: BTreeMap::new(),
                },
            ],
            root_dependencies: BTreeMap::from([
                (
                    "a".to_string(),
                    dependency_instance_id("a", "0.0.0", &other),
                ),
                (
                    "z".to_string(),
                    dependency_instance_id("z", "0.0.0", &digest),
                ),
            ]),
        };
        let a = lock.to_yaml();
        let b = lock.to_yaml();
        assert_eq!(a, b);
        assert!(a.find("name: a").unwrap() < a.find("name: z").unwrap());
        let parsed = LockFile::from_yaml(&a).unwrap();
        assert_eq!(parsed.lock_version, LOCK_VERSION);
        assert_eq!(parsed.packages.len(), 2);
    }

    #[test]
    fn v1_and_v2_locks_are_rejected_with_a_relock_hint() {
        for version in [1, 2] {
            let yaml = format!("lock_version: {version}\npackages: []\n");
            let error = LockFile::from_yaml(&yaml).unwrap_err();
            assert!(error.contains("lock v3 is required"), "{error}");
            assert!(error.contains("marqdo lock"), "{error}");
        }
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
        std::fs::write(first.join("index.mq.md"), "## @exec f\n\n`return 1`\n").unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver =
            PackageResolver::new(root.clone(), Some(manifest.clone()), None).with_unlocked(true);
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
        let resolver = PackageResolver::new(root, Some(manifest), None).with_unlocked(true);
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
        #[cfg(windows)]
        {
            // Rooted but drive-less: `is_absolute()` is false on Windows, yet
            // joining it escapes the project root.
            let rooted =
                "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  x:\n    path: \\tmp\\x\n";
            assert!(Manifest::from_yaml(rooted).is_err());
        }
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
        std::fs::write(dep.join("index.mq.md"), "## @exec hello\n\n`return 1`\n").unwrap();
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
        let resolver = PackageResolver::new(root.clone(), Some(manifest.clone()), None)
            .with_fetch(true)
            .with_unlocked(true);
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
        let offline = PackageResolver::new(root, Some(manifest), None).with_unlocked(true);
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
        std::fs::write(dep.join("index.mq.md"), "## @exec area\n\n`return 1`\n").unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  geometry:\n    path: ./deps/geometry\n",
        )
        .unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver = PackageResolver::new(root.clone(), Some(manifest.clone()), None)
            .with_fetch(true)
            .with_unlocked(true);
        let graph = resolver.resolve_lock_graph().unwrap();
        let resolved = graph
            .packages
            .values()
            .find(|package| package.instance_id != graph.lock.root.package)
            .unwrap()
            .clone();
        let lock = graph.lock.clone();
        resolver.materialize_env(&lock).unwrap();
        let env = root.join(".marqdo/env");
        assert!(env.join("env.json").is_file());
        assert!(env
            .join(env_dir_name(&resolved.instance_id))
            .join("index.mq.md")
            .is_file());
        // A resolver with the lock reads the materialized copy.
        let read = PackageResolver::new(root, Some(manifest), Some(lock));
        let again = read.resolve_alias("geometry").unwrap();
        assert_eq!(again.package.instance_id, resolved.instance_id);
        assert!(again.package.module_root.starts_with(env));
    }

    #[test]
    fn lock_graph_records_transitive_path_closure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let a = root.join("deps/a");
        let b = a.join("b");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  a:\n    path: ./deps/a\n",
        )
        .unwrap();
        std::fs::write(root.join("index.mq.md"), "## @exec main\n\n`return 1`\n").unwrap();
        std::fs::write(
            a.join(MANIFEST_FILE),
            "project:\n  name: a\n  version: 0.1.0\ndependencies:\n  b:\n    path: ./b\n",
        )
        .unwrap();
        std::fs::write(a.join("index.mq.md"), "## @exec a\n\n`return 1`\n").unwrap();
        std::fs::write(
            b.join(MANIFEST_FILE),
            "project:\n  name: b\n  version: 0.1.0\n",
        )
        .unwrap();
        std::fs::write(b.join("index.mq.md"), "## @exec b\n\n`return 2`\n").unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver = PackageResolver::new(root.clone(), Some(manifest), None).with_unlocked(true);
        let graph = resolver.resolve_lock_graph().unwrap();
        assert_eq!(graph.lock.packages.len(), 2);
        assert_eq!(graph.lock.root_dependencies.len(), 1);
        let a_record = graph
            .lock
            .packages
            .iter()
            .find(|package| package.name == "a")
            .unwrap();
        assert!(a_record.dependencies.contains_key("b"));
        let b_record = graph
            .lock
            .packages
            .iter()
            .find(|package| package.name == "b")
            .unwrap();
        assert_eq!(
            a_record.dependencies.get("b").unwrap(),
            &b_record.instance_id
        );
    }

    #[test]
    fn missing_locked_edge_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let dep = root.join("deps/geometry");
        std::fs::create_dir_all(&dep).unwrap();
        std::fs::write(
            dep.join(MANIFEST_FILE),
            "project:\n  name: geometry\n  version: 0.1.0\n",
        )
        .unwrap();
        std::fs::write(dep.join("index.mq.md"), "## @exec area\n\n`return 1`\n").unwrap();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\ndependencies:\n  geometry:\n    path: ./deps/geometry\n",
        )
        .unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver =
            PackageResolver::new(root.clone(), Some(manifest.clone()), None).with_unlocked(true);
        let mut lock = resolver.resolve_lock_graph().unwrap().lock;
        lock.root_dependencies.clear();
        let read = PackageResolver::new(root, Some(manifest), Some(lock));
        let error = read.resolve_alias("geometry").unwrap_err();
        assert!(error.contains("not recorded in marqdo.lock"), "{error}");
    }

    #[test]
    fn root_manifest_edit_invalidates_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "project:\n  name: p\n  version: 0.1.0\n",
        )
        .unwrap();
        let manifest = Manifest::load(&root.join(MANIFEST_FILE)).unwrap();
        let resolver = PackageResolver::new(root.to_path_buf(), Some(manifest.clone()), None)
            .with_unlocked(true);
        let lock = resolver.resolve_lock_graph().unwrap().lock;
        let mut edited = manifest;
        edited.capabilities.process = Some(true);
        let error = lock.validate_root(&edited).unwrap_err();
        assert!(error.contains("root manifest changed"), "{error}");
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
        // macOS temp_dir is `/var/folders/...` while canonicalize resolves the
        // `/var -> /private/var` symlink; compare canonical forms on both sides.
        assert_eq!(
            discover_root(&nested).unwrap().canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert!(discover_root(Path::new("/")).is_none());
    }
}
