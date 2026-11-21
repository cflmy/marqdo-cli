//! Static extension descriptors for the CLI composition root
//! (RFC-EXT-0001 §4.4/§6, ADR-0054/0056).
//!
//! This module only *reads* manifests. It never `dlopen`s a library, never
//! spawns a worker, never opens a network connection and never executes a
//! side-effect script. The descriptors produced here feed the single
//! [`marqdo_ir::callable::CallableRegistry`] and binding pass; `run` later
//! verifies the real artifacts against these recorded identities.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use marqdo_ir::callable::{CallableDescriptor, CallableId, CallableOrigin, CallableSignature};
use marqdo_ir::SignatureKind;
use marqdo_plugin_abi::{HostTarget, PluginKind, PluginManifest};
use marqdo_span::Span;
use sha2::{Digest, Sha256};

/// A structured prepare-stage failure.
#[derive(Debug, Clone)]
pub struct PrepareError {
    /// Stable diagnostic code (`ext.*` unless reusing an existing event code).
    pub code: String,
    /// Human-readable message; never contains secrets or binary content.
    pub message: String,
    /// CLI exit code (2 for manifest/option errors, 1 for load/verify errors).
    pub exit_code: i32,
}

impl PrepareError {
    /// Construct a manifest/option error (exit code 2).
    pub fn option(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 2,
        }
    }

    /// Construct a loading/verification error (exit code 1).
    pub fn load(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            exit_code: 1,
        }
    }
}

/// One statically declared export.
#[derive(Debug, Clone)]
pub struct StaticPluginExport {
    /// Complete dotted export name.
    pub name: String,
    /// Raw manifest signature string, compared exactly at run time.
    pub signature: String,
    /// Parsed static descriptor used by the binding pass.
    pub descriptor: CallableDescriptor,
}

/// One statically read plugin package.
///
/// The identity fields are recorded for `inspect --extensions`, evidence and
/// reconciliation; they are not consulted for name resolution (the binding
/// table is).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StaticPlugin {
    /// Canonical manifest path.
    pub manifest_path: PathBuf,
    /// Package directory (artifact root).
    pub base_dir: PathBuf,
    /// Declared package name.
    pub package: String,
    /// Declared package version.
    pub version: String,
    /// Deployment kind (`native`, `worker`, `wasm`).
    pub kind: PluginKind,
    /// `sha256:<hex>` of the manifest file bytes.
    pub manifest_sha256: String,
    /// Target triple selected for this host.
    pub artifact_target: String,
    /// Declared (not yet verified) artifact sha256, lowercase hex.
    pub artifact_sha256: String,
    /// Bundle-relative artifact path.
    pub artifact_relative: String,
    /// Exports in manifest order.
    pub exports: Vec<StaticPluginExport>,
    /// Declared capability requests (metadata only; never grants).
    pub requires: Vec<String>,
}

impl StaticPlugin {
    /// Absolute artifact path inside the package bundle.
    pub fn artifact_path(&self) -> PathBuf {
        self.base_dir.join(&self.artifact_relative)
    }
}

/// Static plugin selection read from explicit CLI `--plugin` flags.
#[derive(Debug, Clone, Default)]
pub struct StaticPluginSet {
    /// Packages in deterministic manifest order.
    pub plugins: Vec<StaticPlugin>,
}

impl StaticPluginSet {
    /// All export descriptors in deterministic order.
    pub fn descriptors(&self) -> Vec<CallableDescriptor> {
        let mut descriptors = Vec::new();
        for plugin in &self.plugins {
            for export in &plugin.exports {
                descriptors.push(export.descriptor.clone());
            }
        }
        descriptors
    }
}

/// Read and validate all explicitly selected `--plugin` manifests.
///
/// No binary is opened or executed. Duplicate package identities, duplicate
/// full export names, reserved names and malformed signatures are rejected
/// here, before any binding is produced.
pub fn read_plugin_descriptors(paths: &[PathBuf]) -> Result<StaticPluginSet, PrepareError> {
    let mut set = StaticPluginSet::default();
    let mut export_names: BTreeSet<String> = BTreeSet::new();
    let mut packages: BTreeSet<(String, String)> = BTreeSet::new();
    for path in paths {
        let manifest_path = if path.is_dir() {
            path.join("plugin.yaml")
        } else {
            path.clone()
        };
        let manifest_path = manifest_path.canonicalize().map_err(|error| {
            PrepareError::option(
                "ext.manifest_unreadable",
                format!(
                    "cannot open plugin manifest `{}`: {error}",
                    manifest_path.display()
                ),
            )
        })?;
        let base_dir = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let base_dir = base_dir.canonicalize().unwrap_or(base_dir);
        let manifest_bytes = std::fs::read(&manifest_path).map_err(|error| {
            PrepareError::option(
                "ext.manifest_unreadable",
                format!(
                    "cannot read plugin manifest `{}`: {error}",
                    manifest_path.display()
                ),
            )
        })?;
        let manifest = PluginManifest::load(&manifest_path)
            .map_err(|error| PrepareError::option(error.code.clone(), error.message.clone()))?;
        manifest
            .validate()
            .map_err(|error| PrepareError::option(error.code.clone(), error.message.clone()))?;
        let host = HostTarget::current();
        let artifact = manifest.resolve_artifact(&host).map_err(|error| {
            PrepareError::option("ext.no_matching_artifact", error.message.clone())
        })?;
        let artifact_relative = artifact.file.clone();
        ensure_bundle_relative(&artifact_relative).map_err(|error| {
            PrepareError::option(
                "ext.artifact_integrity",
                format!(
                    "plugin `{}` artifact path `{artifact_relative}` is invalid: {error}",
                    manifest.plugin.name
                ),
            )
        })?;
        let manifest_sha256 = format!("sha256:{}", hex(&Sha256::digest(&manifest_bytes)));
        let identity = (manifest.plugin.name.clone(), manifest_sha256.clone());
        if !packages.insert(identity.clone()) {
            // The exact same package declaration twice is idempotent.
            continue;
        }
        if set
            .plugins
            .iter()
            .any(|plugin| plugin.package == identity.0)
        {
            return Err(PrepareError::option(
                "ext.duplicate_callable",
                format!(
                    "plugin package `{}` is selected with two different manifests; \
                     explicit aliases are not implemented, so this is rejected",
                    identity.0
                ),
            ));
        }
        let mut exports = Vec::new();
        for export in &manifest.plugin.exports {
            if !export_names.insert(export.name.clone()) {
                return Err(PrepareError::option(
                    "ext.duplicate_callable",
                    format!(
                        "plugin export `{}` is declared by more than one selected package",
                        export.name
                    ),
                ));
            }
            let signature = plugin_signature(&export.signature).map_err(|error| {
                PrepareError::option(
                    "ext.plugin_invalid_signature",
                    format!(
                        "plugin export `{}` has an invalid signature: {error}",
                        export.name
                    ),
                )
            })?;
            let descriptor = CallableDescriptor {
                id: CallableId(format!("plugin:{}:{}", manifest.plugin.name, export.name)),
                public_name: export.name.clone(),
                owner_package: manifest.plugin.name.clone(),
                signature,
                effects_requested: manifest.plugin.requires.clone(),
                origin: CallableOrigin::AbiPlugin {
                    package: manifest.plugin.name.clone(),
                    export: export.name.clone(),
                },
                implementation_digest: Some(artifact.sha256.clone()),
                source_span: None,
            };
            exports.push(StaticPluginExport {
                name: export.name.clone(),
                signature: export.signature.clone(),
                descriptor,
            });
        }
        set.plugins.push(StaticPlugin {
            manifest_path,
            base_dir,
            package: manifest.plugin.name.clone(),
            version: manifest.plugin.version.clone(),
            kind: manifest.plugin.kind,
            manifest_sha256,
            artifact_target: host.triple().to_string(),
            artifact_sha256: artifact.sha256.clone(),
            artifact_relative,
            exports,
            requires: manifest.plugin.requires.clone(),
        });
    }
    set.plugins.sort_by(|left, right| {
        (&left.package, &left.manifest_sha256).cmp(&(&right.package, &right.manifest_sha256))
    });
    Ok(set)
}

/// Convert a plugin manifest signature (`($a: Int, $b: Int) -> Int`) to the
/// language parameter list.
///
/// ABI v1 transports named arguments as a map, so positional arguments are a
/// static-validated convenience: the binding pass normalises them to the
/// manifest order. Default values are rejected until the ABI wire semantics for
/// them are versioned (RFC-EXT-0001 §4.5.3); silently injecting defaults at run
/// time is forbidden.
pub fn plugin_signature(raw: &str) -> Result<CallableSignature, String> {
    let before_return = raw.split("->").next().unwrap_or(raw).trim();
    let open = before_return
        .find('(')
        .ok_or_else(|| "missing `(`".to_string())?;
    let close = before_return
        .rfind(')')
        .ok_or_else(|| "missing `)`".to_string())?;
    if close < open {
        return Err("malformed parameter list".to_string());
    }
    let params = before_return[open + 1..close].replace('$', "");
    let parsed = marqdo_frontend::signature::parse(&format!("({params})"), Span::default());
    if parsed.has_errors() {
        return Err(parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
            .join("; "));
    }
    if parsed.params.iter().any(|param| param.default.is_some()) {
        return Err(
            "ABI v1 plugin signatures must not declare default values; make parameters required"
                .to_string(),
        );
    }
    Ok(CallableSignature {
        params: parsed.params,
        kind: SignatureKind::Explicit,
    })
}

/// Reject absolute paths, `..` components and empty paths.
pub fn ensure_bundle_relative(path: &str) -> Result<(), String> {
    let mut normalised = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalised.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("must be a relative path inside the package".to_string())
            }
        }
    }
    if normalised.as_os_str().is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(())
}

/// Verify a bundle file is a regular file below `bundle_root`, travelling
/// through symlinks only if the final canonical target stays inside the bundle.
pub fn verify_bundle_regular_file(
    bundle_root: &Path,
    relative: &str,
) -> Result<PathBuf, PrepareError> {
    ensure_bundle_relative(relative).map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!("artifact path `{relative}` is invalid: {error}"),
        )
    })?;
    let root = bundle_root.canonicalize().map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!(
                "cannot resolve package root `{}`: {error}",
                bundle_root.display()
            ),
        )
    })?;
    let candidate = root.join(relative);
    let canonical = candidate.canonicalize().map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!("cannot open artifact `{}`: {error}", candidate.display()),
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(PrepareError::load(
            "ext.artifact_integrity",
            format!(
                "artifact `{relative}` resolves outside its package bundle (`{}`)",
                canonical.display()
            ),
        ));
    }
    let metadata = std::fs::metadata(&canonical).map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!("cannot stat artifact `{}`: {error}", canonical.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(PrepareError::load(
            "ext.artifact_integrity",
            format!("artifact `{}` is not a regular file", canonical.display()),
        ));
    }
    Ok(canonical)
}

/// Verify a file's lowercase sha256 against a declared value.
pub fn verify_sha256(path: &Path, declared: &str) -> Result<(), PrepareError> {
    let bytes = std::fs::read(path).map_err(|error| {
        PrepareError::load(
            "ext.artifact_integrity",
            format!("cannot read artifact `{}`: {error}", path.display()),
        )
    })?;
    let actual = hex(&Sha256::digest(&bytes));
    if actual != declared.to_ascii_lowercase() {
        return Err(PrepareError::load(
            "ext.artifact_integrity",
            format!(
                "artifact `{}` sha256 mismatch: declared {declared}, actual {actual}",
                path.display()
            ),
        ));
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

    #[test]
    fn plugin_signature_rejects_defaults_and_parses_positional() {
        assert!(plugin_signature("($a: Int, $b: Int) -> Int").is_ok());
        let error = plugin_signature("($a: Int = 1) -> Int").unwrap_err();
        assert!(error.contains("default"), "{error}");
    }

    #[test]
    fn bundle_relative_paths_reject_traversal() {
        assert!(ensure_bundle_relative("lib/demo.so").is_ok());
        assert!(ensure_bundle_relative("../demo.so").is_err());
        assert!(ensure_bundle_relative("/etc/passwd").is_err());
        assert!(ensure_bundle_relative("").is_err());
    }
}
