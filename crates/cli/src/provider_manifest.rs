//! Strict third-party Resource Provider v2 manifests for the CLI composition
//! root (RFC-EXT-0001 §5.3, ADR-0055).
//!
//! This module *reads* `provider.json` schema version 2 and verifies the
//! selected artifact. It never `dlopen`s a library, never spawns a helper and
//! never authorizes a provider: parsing a manifest is not a grant. The v1
//! `marqdo.provider` manifest stays a host-embedding path
//! (`StdHost::register_provider_package`, ADR-0037) and is rejected here with
//! `ext.unsupported_provider_schema`; there is no silent migration.
//!
//! The CLI only accepts the `c-abi-1` + `worker` transport combination and
//! resolves the helper as a sibling of the running `marqdo` executable; PATH
//! lookup is never used. Integrity hashes are content-integrity checks, not
//! publisher authentication.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use marqdo_host::StdHost;
use marqdo_plugin_abi::{HostTarget, TargetTriple};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::extension_descriptors::{
    ensure_bundle_relative, verify_bundle_regular_file, verify_sha256, PrepareError,
};

/// Provider manifest file name used by the CLI (`--provider` accepts this file).
pub const PROVIDER_MANIFEST_FILE: &str = "provider.json";

/// Provider manifest schema name.
pub const PROVIDER_SCHEMA: &str = "marqdo.provider";
/// Provider manifest schema version accepted by the CLI.
pub const PROVIDER_SCHEMA_VERSION: u64 = 2;
/// Provider ABI accepted by the CLI (ADR-0036/0038).
pub const PROVIDER_ABI: &str = "c-abi-1";
/// Provider transport accepted by the CLI (ADR-0035/0038).
pub const PROVIDER_TRANSPORT: &str = "worker";
/// Helper executable shipped next to the `marqdo` binary (ADR-0038).
pub const PROVIDER_WORKER_BINARY: &str = "marqdo-provider-worker";

/// Upper bounds for a single provider manifest; these bound parser work only.
pub const MAX_PROVIDER_NAME_BYTES: usize = 128;
pub const MAX_PROVIDER_KINDS: usize = 64;
pub const MAX_PROVIDER_ARTIFACTS: usize = 32;
pub const MAX_PROVIDER_PERMISSIONS: usize = 64;
/// Maximum selected artifact size (256 MiB).
pub const MAX_PROVIDER_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum manifest size (1 MiB); a manifest is metadata, not an artifact.
pub const MAX_PROVIDER_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Core resource kinds owned by the host; third parties may never override
/// them (RFC-EXT-0001 §5.3.1). `stdio`/`file`/`tcp`/`tls`/`udp`/`tcp-listener`
/// are dispatched by `marqdo-host` before the provider registry is consulted.
pub const RESERVED_RESOURCE_KINDS: &[&str] =
    &["stdio", "file", "tcp", "tls", "udp", "tcp-listener"];

/// Raw `provider.json` schema version 2 document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderManifest {
    schema: String,
    schema_version: u64,
    name: String,
    version: String,
    abi: String,
    transport: String,
    kinds: Vec<String>,
    artifacts: Vec<ManifestArtifact>,
    #[serde(default)]
    permissions_requested: Vec<String>,
    /// Free-form, non-executable metadata. Ignored for execution and never used
    /// to select artifacts or grant permissions.
    #[serde(rename = "metadata", default)]
    _metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestArtifact {
    target: String,
    path: String,
    sha256: String,
}

/// A manifest whose selected artifact has been resolved and verified.
///
/// Producing one requires reading the manifest, checking the schema rules,
/// selecting the artifact for the current host target and verifying the
/// content hash. It still does not grant access and no worker has been started.
#[derive(Debug, Clone)]
pub struct VerifiedProvider {
    /// Stable grant name (`--allow-provider NAME` must match exactly).
    pub name: String,
    /// Strict SemVer version declared by the manifest.
    pub version: String,
    /// Non-reserved resource kinds served by this provider.
    pub kinds: Vec<String>,
    /// Canonical manifest path.
    pub manifest_path: PathBuf,
    /// Canonical bundle directory (manifest parent).
    pub base_dir: PathBuf,
    /// `sha256:<hex>` of the manifest bytes at verification time.
    pub manifest_digest: String,
    /// Selected transport (`worker` in v2).
    pub transport: String,
    /// Canonical path of the selected, hash-verified artifact.
    pub selected_artifact_path: PathBuf,
    /// Bundle-relative artifact path exactly as declared.
    pub selected_artifact_relative: String,
    /// Lowercase hex sha256 declared by the manifest.
    pub selected_artifact_sha256: String,
    /// Target triple the selected artifact was built for.
    pub selected_target: String,
}

impl VerifiedProvider {
    /// Re-verify the manifest bytes and the selected artifact immediately
    /// before the provider worker is spawned.
    ///
    /// `run` must never trust a hash it checked earlier: another process can
    /// replace the manifest or the artifact between selection and use.
    pub fn reverify(&self) -> Result<(), PrepareError> {
        let bytes = std::fs::read(&self.manifest_path).map_err(|error| {
            PrepareError::load(
                "ext.artifact_integrity",
                format!(
                    "provider manifest `{}` could not be re-read before use: {error}",
                    self.manifest_path.display()
                ),
            )
        })?;
        if bytes.len() as u64 > MAX_PROVIDER_MANIFEST_BYTES {
            return Err(PrepareError::load(
                "ext.artifact_integrity",
                format!(
                    "provider manifest `{}` grew beyond the {MAX_PROVIDER_MANIFEST_BYTES}-byte limit before use",
                    self.manifest_path.display()
                ),
            ));
        }
        let digest = manifest_digest(&bytes);
        if digest != self.manifest_digest {
            return Err(PrepareError::load(
                "ext.artifact_integrity",
                format!(
                    "provider manifest `{}` changed after verification (expected {}, actual {}); refusing to start its worker",
                    self.manifest_path.display(),
                    self.manifest_digest,
                    digest
                ),
            ));
        }
        let artifact =
            verify_bundle_regular_file(&self.base_dir, &self.selected_artifact_relative)?;
        if artifact != self.selected_artifact_path {
            return Err(PrepareError::load(
                "ext.artifact_integrity",
                format!(
                    "provider `{}` artifact `{}` now resolves to `{}` instead of the verified `{}`",
                    self.name,
                    self.selected_artifact_relative,
                    artifact.display(),
                    self.selected_artifact_path.display()
                ),
            ));
        }
        check_artifact_size(&artifact)?;
        verify_sha256(&artifact, &self.selected_artifact_sha256)?;
        Ok(())
    }
}

/// Read, validate and verify every explicitly selected `--provider` manifest.
///
/// No worker is spawned and no artifact is executed. Duplicate provider names
/// and duplicate resource kinds across providers are rejected deterministically
/// (the returned list is sorted by provider name) so a later registration can
/// never overwrite an earlier one.
pub fn read_provider_manifests(paths: &[PathBuf]) -> Result<Vec<VerifiedProvider>, PrepareError> {
    let mut manifest_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for path in paths {
        if path.is_dir() {
            return Err(PrepareError::option(
                "ext.manifest_unreadable",
                format!(
                    "--provider `{}` must name a `{PROVIDER_MANIFEST_FILE}` file, not a directory",
                    path.display()
                ),
            ));
        }
        let canonical = path.canonicalize().map_err(|error| {
            PrepareError::option(
                "ext.manifest_unreadable",
                format!(
                    "cannot open provider manifest `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        manifest_paths.insert(canonical);
    }

    let mut providers = Vec::with_capacity(manifest_paths.len());
    for path in manifest_paths {
        providers.push(read_provider_manifest(&path)?);
    }

    // Deterministic conflict resolution independent of CLI argument order.
    providers.sort_by(|left, right| {
        (&left.name, &left.manifest_digest).cmp(&(&right.name, &right.manifest_digest))
    });
    let mut unique: Vec<VerifiedProvider> = Vec::with_capacity(providers.len());
    for provider in providers {
        if let Some(existing) = unique.iter().find(|item| item.name == provider.name) {
            if existing.manifest_digest != provider.manifest_digest {
                return Err(PrepareError::option(
                    "ext.duplicate_provider",
                    format!(
                        "provider name `{}` is selected with two different manifests (`{}` and `{}`); \
                         explicit aliases are not implemented, so this is rejected",
                        provider.name,
                        existing.manifest_path.display(),
                        provider.manifest_path.display()
                    ),
                ));
            }
            // The exact same declaration selected twice is idempotent.
            continue;
        }
        unique.push(provider);
    }

    let mut claimed_kinds: BTreeMap<String, String> = BTreeMap::new();
    for provider in &unique {
        for kind in &provider.kinds {
            if let Some(owner) = claimed_kinds.get(kind) {
                return Err(PrepareError::option(
                    "ext.duplicate_provider_kind",
                    format!(
                        "resource kind `{kind}` is declared by both provider `{owner}` and provider `{}`; \
                         duplicate kinds are rejected before any worker starts",
                        provider.name
                    ),
                ));
            }
            claimed_kinds.insert(kind.clone(), provider.name.clone());
        }
    }

    Ok(unique)
}

fn read_provider_manifest(canonical_path: &Path) -> Result<VerifiedProvider, PrepareError> {
    let metadata = std::fs::metadata(canonical_path).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "cannot stat provider manifest `{}`: {error}",
                canonical_path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_PROVIDER_MANIFEST_BYTES {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!(
                "provider manifest `{}` is {} bytes, larger than the {MAX_PROVIDER_MANIFEST_BYTES}-byte limit",
                canonical_path.display(),
                metadata.len()
            ),
        ));
    }
    let bytes = std::fs::read(canonical_path).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "cannot read provider manifest `{}`: {error}",
                canonical_path.display()
            ),
        )
    })?;
    let digest = manifest_digest(&bytes);
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        PrepareError::option(
            "ext.manifest_invalid",
            format!(
                "provider manifest `{}` is not valid UTF-8 JSON: {error}",
                canonical_path.display()
            ),
        )
    })?;

    // Schema/version gate runs before structural parsing so a v1 (or future)
    // manifest gets the stable `ext.unsupported_provider_schema` code instead
    // of a generic field error.
    let schema = value.get("schema").and_then(serde_json::Value::as_str);
    if schema != Some(PROVIDER_SCHEMA) {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider manifest `{}` does not declare `schema: \"{PROVIDER_SCHEMA}\"`; \
                 the CLI accepts schema version {PROVIDER_SCHEMA_VERSION} only",
                canonical_path.display()
            ),
        ));
    }
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64);
    if schema_version != Some(PROVIDER_SCHEMA_VERSION) {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider manifest `{}` declares schema_version {}; the CLI accepts {PROVIDER_SCHEMA_VERSION} only \
                 (v1 and future versions are rejected, there is no silent migration)",
                canonical_path.display(),
                schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "<missing>".to_string())
            ),
        ));
    }

    let manifest: ProviderManifest = serde_json::from_value(value).map_err(|error| {
        PrepareError::option(
            "ext.manifest_invalid",
            format!(
                "provider manifest `{}` is not a valid {PROVIDER_SCHEMA} v{PROVIDER_SCHEMA_VERSION} document: {error}",
                canonical_path.display()
            ),
        )
    })?;
    if manifest.schema != PROVIDER_SCHEMA || manifest.schema_version != PROVIDER_SCHEMA_VERSION {
        // Unreachable after the Value-level gate; kept fail-closed.
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider manifest `{}` uses an unsupported schema",
                canonical_path.display()
            ),
        ));
    }
    validate_manifest(&manifest, canonical_path)?;

    let base_dir = canonical_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let selected_target = HostTarget::current().triple().to_string();
    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.target == selected_target)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.no_matching_artifact",
                format!(
                    "provider `{}` has no artifact for the current target `{selected_target}`; \
                     other platforms are never substituted",
                    manifest.name
                ),
            )
        })?;

    let artifact_relative = normalise_artifact_path(&manifest.name, &artifact.path)?;
    let artifact_path = verify_bundle_regular_file(&base_dir, &artifact_relative)?;
    check_artifact_size(&artifact_path)?;
    verify_sha256(&artifact_path, &artifact.sha256)?;

    Ok(VerifiedProvider {
        name: manifest.name,
        version: manifest.version,
        kinds: manifest.kinds,
        manifest_path: canonical_path.to_path_buf(),
        base_dir,
        manifest_digest: digest,
        transport: manifest.transport,
        selected_artifact_path: artifact_path,
        selected_artifact_relative: artifact_relative,
        selected_artifact_sha256: artifact.sha256.to_ascii_lowercase(),
        selected_target,
    })
}

fn validate_manifest(
    manifest: &ProviderManifest,
    manifest_path: &Path,
) -> Result<(), PrepareError> {
    let invalid = |message: String| {
        PrepareError::option(
            "ext.manifest_invalid",
            format!("provider manifest `{}`: {message}", manifest_path.display()),
        )
    };

    validate_provider_name(&manifest.name)
        .map_err(|error| invalid(format!("invalid provider name: {error}")))?;
    if RESERVED_RESOURCE_KINDS.contains(&manifest.name.as_str()) {
        return Err(invalid(format!(
            "provider name `{}` is a reserved core resource kind",
            manifest.name
        )));
    }
    semver::Version::parse(&manifest.version).map_err(|error| {
        invalid(format!(
            "version `{}` is not strict SemVer: {error}",
            manifest.version
        ))
    })?;
    if manifest.abi != PROVIDER_ABI {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider `{}` declares abi `{}`; schema v{PROVIDER_SCHEMA_VERSION} accepts `{PROVIDER_ABI}` only",
                manifest.name, manifest.abi
            ),
        ));
    }
    if manifest.transport != PROVIDER_TRANSPORT {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider `{}` declares transport `{}`; schema v{PROVIDER_SCHEMA_VERSION} accepts `{PROVIDER_TRANSPORT}` only",
                manifest.name, manifest.transport
            ),
        ));
    }
    if manifest.kinds.is_empty() {
        return Err(invalid(
            "kinds must declare at least one resource kind".to_string(),
        ));
    }
    if manifest.kinds.len() > MAX_PROVIDER_KINDS {
        return Err(invalid(format!(
            "kinds declares more than {MAX_PROVIDER_KINDS} entries"
        )));
    }
    let mut kinds = BTreeSet::new();
    for kind in &manifest.kinds {
        validate_provider_name(kind)
            .map_err(|error| invalid(format!("invalid resource kind `{kind}`: {error}")))?;
        if RESERVED_RESOURCE_KINDS.contains(&kind.as_str()) {
            return Err(invalid(format!(
                "resource kind `{kind}` is reserved by the host and cannot be overridden by a provider"
            )));
        }
        if !kinds.insert(kind.clone()) {
            return Err(invalid(format!(
                "resource kind `{kind}` is declared more than once"
            )));
        }
    }
    if manifest.artifacts.is_empty() {
        return Err(invalid(
            "artifacts must declare at least one target artifact".to_string(),
        ));
    }
    if manifest.artifacts.len() > MAX_PROVIDER_ARTIFACTS {
        return Err(invalid(format!(
            "artifacts declares more than {MAX_PROVIDER_ARTIFACTS} entries"
        )));
    }
    let mut targets = BTreeSet::new();
    for artifact in &manifest.artifacts {
        let triple = TargetTriple::parse(&artifact.target).map_err(|error| {
            invalid(format!(
                "artifact target `{}` is not a target triple: {error}",
                artifact.target
            ))
        })?;
        if !targets.insert(triple.to_string()) {
            return Err(invalid(format!(
                "artifact target `{triple}` is declared more than once"
            )));
        }
        if !is_lowercase_sha256(&artifact.sha256) {
            return Err(invalid(format!(
                "artifact for target `{:?}` must declare a 64-character lowercase hex sha256",
                artifact.target
            )));
        }
    }
    if manifest.permissions_requested.len() > MAX_PROVIDER_PERMISSIONS {
        return Err(invalid(format!(
            "permissions_requested declares more than {MAX_PROVIDER_PERMISSIONS} entries"
        )));
    }
    let mut permissions = BTreeSet::new();
    for permission in &manifest.permissions_requested {
        if !is_known_capability(permission) && !is_vendor_capability(permission) {
            return Err(invalid(format!(
                "permissions_requested entry `{permission}` is not a known capability or a dotted vendor capability"
            )));
        }
        if !permissions.insert(permission.clone()) {
            return Err(invalid(format!(
                "permission `{permission}` is declared more than once"
            )));
        }
    }
    // `metadata` is intentionally not validated: it has no execution meaning.
    Ok(())
}

/// Validate a dotted provider/kind name using the plugin name grammar
/// (ASCII lowercase segment start, then lowercase/digits/`-`/`_`).
pub fn validate_provider_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("must not be empty".to_string());
    }
    if name.len() > MAX_PROVIDER_NAME_BYTES {
        return Err(format!(
            "must not be longer than {MAX_PROVIDER_NAME_BYTES} bytes"
        ));
    }
    for segment in name.split('.') {
        if segment.is_empty() {
            return Err(format!("`{name}` contains an empty segment"));
        }
        let mut bytes = segment.bytes();
        let first = bytes.next().unwrap_or(b'a');
        if !first.is_ascii_lowercase() {
            return Err(format!(
                "`{name}` must start every `.`-separated segment with a lowercase ASCII letter"
            ));
        }
        if !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        }) {
            return Err(format!(
                "`{name}` may only contain lowercase ASCII letters, digits, `-` and `_`"
            ));
        }
    }
    Ok(())
}

/// Reject absolute paths and `..` traversal in declared artifact paths.
fn normalise_artifact_path(provider: &str, path: &str) -> Result<String, PrepareError> {
    ensure_bundle_relative(path).map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!("provider `{provider}` artifact path `{path}` is invalid: {error}"),
        )
    })?;
    Ok(path.to_string())
}

fn check_artifact_size(path: &Path) -> Result<(), PrepareError> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!(
                "cannot stat provider artifact `{}`: {error}",
                path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_PROVIDER_ARTIFACT_BYTES {
        return Err(PrepareError::load(
            "ext.artifact_integrity",
            format!(
                "provider artifact `{}` is {} bytes, larger than the {MAX_PROVIDER_ARTIFACT_BYTES}-byte limit",
                path.display(),
                metadata.len()
            ),
        ));
    }
    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_known_capability(name: &str) -> bool {
    marqdo_plugin_abi::KNOWN_CAPABILITIES.contains(&name)
}

/// A dotted vendor capability proposal (validation only, never a grant).
fn is_vendor_capability(name: &str) -> bool {
    let segments: Vec<&str> = name.split('.').collect();
    if segments.len() < 3 {
        return false;
    }
    if matches!(
        segments[0],
        "console" | "math" | "fs" | "net" | "process" | "log" | "clock" | "random"
    ) {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
            })
    })
}

fn manifest_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("sha256:{hex}")
}

/// Whether the provider `worker` transport is supported on this build.
///
/// RFC-EXT-0001 §5.6 makes Linux x86_64 the mandatory lane. Other targets stay
/// compilable but must fail closed with `ext.platform_unsupported` instead of
/// pretending a helper works.
pub fn provider_worker_platform_supported() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
}

/// Resolve the provider helper shipped next to the running `marqdo` executable.
///
/// Resolution is *only* the sibling file: there is no PATH lookup and no
/// directory search (RFC-EXT-0001 §5.4.5). A missing helper is
/// `ext.worker_unavailable`, never a fallback to in-process native loading.
pub fn provider_worker_executable() -> Result<PathBuf, PrepareError> {
    if !provider_worker_platform_supported() {
        return Err(PrepareError::load(
            "ext.platform_unsupported",
            format!(
                "the resource provider worker transport is supported on linux/x86_64 only in this release; \
                 this build targets {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        ));
    }
    let executable = std::env::current_exe().map_err(|error| {
        PrepareError::load(
            "ext.worker_unavailable",
            format!(
                "cannot locate the marqdo executable to find `{PROVIDER_WORKER_BINARY}`: {error}"
            ),
        )
    })?;
    let directory = executable.parent().ok_or_else(|| {
        PrepareError::load(
            "ext.worker_unavailable",
            format!(
                "the marqdo executable `{}` has no parent directory",
                executable.display()
            ),
        )
    })?;
    let helper = directory.join(format!(
        "{PROVIDER_WORKER_BINARY}{}",
        std::env::consts::EXE_SUFFIX
    ));
    match std::fs::metadata(&helper) {
        Ok(metadata) if metadata.is_file() => Ok(helper),
        _ => Err(PrepareError::load(
            "ext.worker_unavailable",
            format!(
                "the resource provider worker helper `{}` is missing; it must be shipped next to the \
                 marqdo executable (PATH lookup is not allowed)",
                helper.display()
            ),
        )),
    }
}

/// Check the exact `--allow-provider` grant, re-verify the manifest and
/// artifact, then register the existing worker resource-provider transport.
///
/// The grant check is repeated here (it was already checked during pre-spawn
/// verification) so no caller can register an ungranted provider. The provider
/// is always registered through
/// [`StdHost::register_worker_resource_provider`]; this path never falls back
/// to the trusted in-process C ABI or a native library.
pub fn load_resource_provider_into_host(
    host: &mut StdHost,
    provider: &VerifiedProvider,
    allow_provider: &[String],
) -> Result<(), PrepareError> {
    if !allow_provider.iter().any(|grant| grant == &provider.name) {
        return Err(PrepareError::load(
            "permission.denied",
            format!(
                "resource provider `{}` was selected but not granted; add --allow-provider {}",
                provider.name, provider.name
            ),
        ));
    }
    if provider.transport != PROVIDER_TRANSPORT {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "provider `{}` transport `{}` cannot be loaded by the CLI",
                provider.name, provider.transport
            ),
        ));
    }
    // Re-verify immediately before use; never trust an earlier hash.
    provider.reverify()?;
    let worker = provider_worker_executable()?;
    let kinds: Vec<&str> = provider.kinds.iter().map(String::as_str).collect();
    let args = vec![provider
        .selected_artifact_path
        .to_string_lossy()
        .into_owned()];
    host.register_worker_resource_provider(&provider.name, &kinds, &worker, &args)
        .map_err(|error| {
            PrepareError::load(
                "ext.worker_unavailable",
                format!(
                "cannot start the worker for resource provider `{}` v{} (target {}) using `{}`: {}",
                provider.name,
                provider.version,
                provider.selected_target,
                worker.display(),
                error.message
            ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "marqdo-provider-manifest-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_manifest(dir: &Path, manifest: &str) -> PathBuf {
        let path = dir.join(PROVIDER_MANIFEST_FILE);
        std::fs::write(&path, manifest).unwrap();
        path
    }

    fn artifact(dir: &Path, name: &str, bytes: &[u8]) -> (String, String) {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let mut hex = String::new();
        for byte in hasher.finalize() {
            hex.push_str(&format!("{byte:02x}"));
        }
        (name.to_string(), hex)
    }

    fn manifest_text(name: &str, kinds: &[&str], path: &str, sha: &str) -> String {
        let kinds = kinds
            .iter()
            .map(|kind| format!("\"{kind}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            r#"{{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "{name}",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": [{kinds}],
  "artifacts": [
    {{"target": "{}", "path": "{path}", "sha256": "{sha}"}}
  ],
  "permissions_requested": []
}}"#,
            HostTarget::current().triple()
        )
    }

    #[test]
    fn valid_v2_manifest_verifies_artifact() {
        let dir = tempdir("valid");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let manifest = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &sha),
        );
        let providers = read_provider_manifests(&[manifest]).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "demo.sensor");
        assert_eq!(
            providers[0].selected_target,
            HostTarget::current().triple().to_string()
        );
        assert!(providers[0].manifest_digest.starts_with("sha256:"));
        providers[0].reverify().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_manifest_is_rejected_without_migration() {
        let dir = tempdir("v1");
        let manifest = write_manifest(
            &dir,
            r#"{"schema":"marqdo.provider","schema_version":1,"name":"cabi","kinds":["cabi.sensor"],"abi":"c-abi-1","library":{"linux":"libcabi.so"}}"#,
        );
        let error = read_provider_manifests(&[manifest]).unwrap_err();
        assert_eq!(error.code, "ext.unsupported_provider_schema");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_target_is_no_matching_artifact() {
        let dir = tempdir("target");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let manifest = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &sha).replace(
                &HostTarget::current().triple().to_string(),
                "aarch64-unknown-linux-gnu",
            ),
        );
        let error = read_provider_manifests(&[manifest]).unwrap_err();
        assert_eq!(error.code, "ext.no_matching_artifact");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reserved_kind_is_rejected() {
        let dir = tempdir("reserved");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let manifest = write_manifest(&dir, &manifest_text("demo.sensor", &["file"], &path, &sha));
        let error = read_provider_manifests(&[manifest]).unwrap_err();
        assert_eq!(error.code, "ext.manifest_invalid");
        assert!(error.message.contains("reserved"), "{}", error.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traversal_and_digest_mismatch_are_artifact_integrity() {
        let dir = tempdir("integrity");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let traversal = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], "../libdemo.so", &sha),
        );
        let error = read_provider_manifests(&[traversal]).unwrap_err();
        assert_eq!(error.code, "ext.artifact_integrity");
        let mismatch = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &"0".repeat(64)),
        );
        let error = read_provider_manifests(&[mismatch]).unwrap_err();
        assert_eq!(error.code, "ext.artifact_integrity");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_kinds_across_providers_are_rejected() {
        let dir = tempdir("duplicate-kind");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let first = write_manifest(
            &dir,
            &manifest_text("demo.one", &["demo.sensor"], &path, &sha),
        );
        let second_dir = dir.join("second");
        std::fs::create_dir_all(&second_dir).unwrap();
        std::fs::copy(dir.join(&path), second_dir.join(&path)).unwrap();
        let second = write_manifest(
            &second_dir,
            &manifest_text("demo.two", &["demo.sensor"], &path, &sha),
        );
        let error = read_provider_manifests(&[first, second]).unwrap_err();
        assert_eq!(error.code, "ext.duplicate_provider_kind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_provider_name_with_different_manifests_is_rejected() {
        let dir = tempdir("duplicate-name");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let first = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &sha),
        );
        let second_dir = dir.join("second");
        std::fs::create_dir_all(&second_dir).unwrap();
        std::fs::copy(dir.join(&path), second_dir.join(&path)).unwrap();
        // Same name, different version => different manifest digest.
        let second = write_manifest(
            &second_dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &sha)
                .replace("\"version\": \"0.1.0\"", "\"version\": \"0.2.0\""),
        );
        let error = read_provider_manifests(&[first.clone(), second]).unwrap_err();
        assert_eq!(error.code, "ext.duplicate_provider");
        // Selecting the exact same manifest twice is idempotent.
        let providers = read_provider_manifests(&[first.clone(), first]).unwrap();
        assert_eq!(providers.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_fields_and_bad_names_are_manifest_invalid() {
        let dir = tempdir("invalid");
        let (path, sha) = artifact(&dir, "libdemo.so", b"provider-bytes");
        let unknown = write_manifest(
            &dir,
            &manifest_text("demo.sensor", &["demo.sensor"], &path, &sha).replace(
                "\"permissions_requested\": []",
                "\"permissions_requested\": [], \"exec\": true",
            ),
        );
        let error = read_provider_manifests(&[unknown]).unwrap_err();
        assert_eq!(error.code, "ext.manifest_invalid");
        let bad_name = write_manifest(
            &dir,
            &manifest_text("Demo.Sensor", &["demo.sensor"], &path, &sha),
        );
        let error = read_provider_manifests(&[bad_name]).unwrap_err();
        assert_eq!(error.code, "ext.manifest_invalid");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_helper_resolution_only_uses_the_executable_sibling() {
        // The real helper is installed next to the CLI binary by the
        // integration fixture; this test only asserts the platform/absence
        // contract and that a path is never derived from PATH.
        if provider_worker_platform_supported() {
            let helper = provider_worker_executable();
            // In a unit-test binary `current_exe()` is the test binary, so the
            // sibling helper is normally absent: the error must be structured.
            match helper {
                Ok(path) => assert!(path.is_file()),
                Err(error) => assert_eq!(error.code, "ext.worker_unavailable"),
            }
        } else {
            let error = provider_worker_executable().unwrap_err();
            assert_eq!(error.code, "ext.platform_unsupported");
        }
    }
}
