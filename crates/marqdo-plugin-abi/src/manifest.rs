//! `plugin.yaml` manifest parsing and validation.
//!
//! A manifest is the declarative half of a plugin package: it names the
//! plugin, pins the ABI generation it targets, declares every export and
//! capability requirement, and lists content-addressed artifacts per target
//! triple. Installing a package never grants permissions; validation never
//! executes plugin code.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::abi::{
    AbiVersion, ABI_MAJOR, ABI_MINOR, MAX_EXPORT_NAME_BYTES, MAX_SIGNATURE_WIRE_BYTES,
};
use crate::artifact::{is_valid_sha256_hex, TargetTriple};
use crate::artifact::{verify_sha256_file, HostTarget};
use crate::error::PluginError;

/// Maximum number of exports a single plugin may register.
pub const MAX_EXPORTS: usize = 256;
/// Maximum number of declared capabilities.
pub const MAX_REQUIRES: usize = 64;
/// Maximum number of artifacts.
pub const MAX_ARTIFACTS: usize = 32;

/// Capability names the host knows about (blueprint section 10.5 whitelist).
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "log",
    "math.compute",
    "fs.read",
    "fs.write",
    "net.request",
    "clock",
    "random",
    "artifact.record",
];

/// Function-name prefixes reserved by the language/host; plugins may not
/// shadow built-ins or other namespaces.
pub const RESERVED_EXPORT_PREFIXES: &[&str] = &["marqdo.", "builtin.", "core.", "std.", "host."];

/// Plugin deployment kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// Native dynamic library loaded in-process. Requires explicit trust.
    Native,
    /// Isolated plugin worker process (same logical plugin API).
    Worker,
    /// Wasm component (future direction; only recognized in v1).
    Wasm,
}

impl PluginKind {
    /// Stable manifest name.
    pub const fn as_str(self) -> &'static str {
        match self {
            PluginKind::Native => "native",
            PluginKind::Worker => "worker",
            PluginKind::Wasm => "wasm",
        }
    }
}

/// One declared export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    /// Dotted export name (e.g. `solver.add`).
    pub name: String,
    /// Marqdo-facing signature string (e.g. `($a: Int, $b: Int) -> Int`).
    pub signature: String,
}

/// One target-specific artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Rust-style target triple (e.g. `x86_64-unknown-linux-gnu`).
    pub target: String,
    /// Relative path to the artifact inside the package.
    pub file: String,
    /// Lowercase hexadecimal sha256 of the artifact contents.
    pub sha256: String,
}

impl Artifact {
    /// Parse the artifact target triple.
    pub fn target_triple(&self) -> Result<TargetTriple, PluginError> {
        TargetTriple::parse(&self.target)
    }

    /// Resolve the artifact path below `base_dir`, rejecting absolute paths
    /// and `..` traversal so a manifest can never point outside its package.
    pub fn resolve_path(&self, base_dir: &Path) -> Result<PathBuf, PluginError> {
        validate_artifact_file(&self.file)?;
        Ok(base_dir.join(&self.file))
    }
}

/// The `plugin:` block of `plugin.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Package name (dotted, e.g. `qdqc.solver`).
    pub name: String,
    /// Package version (semver-ish, e.g. `1.0.0`).
    pub version: String,
    /// Required plugin ABI range (e.g. `^1.0`).
    pub abi: String,
    /// Deployment kind.
    pub kind: PluginKind,
    /// Declared exports.
    pub exports: Vec<Export>,
    /// Declared capability requirements.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Declared artifacts.
    pub artifacts: Vec<Artifact>,
}

/// Parsed `plugin.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// The single `plugin:` block.
    pub plugin: PluginInfo,
}

impl PluginManifest {
    /// Parse a manifest from YAML text.
    pub fn parse_str(yaml: &str) -> Result<Self, PluginError> {
        serde_yaml::from_str(yaml).map_err(|error| {
            PluginError::manifest("manifest_parse_failed", "plugin.yaml could not be parsed")
                .with_detail(error.to_string())
        })
    }

    /// Read and parse a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            PluginError::manifest(
                "manifest_unreadable",
                format!("manifest `{}` could not be read", path.display()),
            )
            .with_detail(error.to_string())
        })?;
        Self::parse_str(&text)
    }

    /// Validate every declared field and cross-field invariant.
    ///
    /// This performs no filesystem access and executes no plugin code.
    pub fn validate(&self) -> Result<(), PluginError> {
        let plugin = &self.plugin;
        validate_dotted_name(&plugin.name, MAX_EXPORT_NAME_BYTES, "plugin name")?;
        if plugin.name.starts_with("marqdo.") || plugin.name.starts_with("builtin.") {
            return Err(PluginError::manifest(
                "reserved_name",
                format!("plugin name `{}` uses a reserved namespace", plugin.name),
            ));
        }
        PackageVersion::parse(&plugin.version)?;
        let requirement = AbiRequirement::parse(&plugin.abi)?;
        if !requirement.is_compatible_with(AbiVersion::current()) {
            return Err(PluginError::unsupported(
                "abi_requirement_unsupported",
                format!(
                    "plugin requires ABI `{}` but the host implements {ABI_MAJOR}.{ABI_MINOR}",
                    plugin.abi
                ),
            ));
        }
        if plugin.exports.is_empty() {
            return Err(PluginError::manifest(
                "no_exports",
                "plugin must declare at least one export",
            ));
        }
        if plugin.exports.len() > MAX_EXPORTS {
            return Err(PluginError::manifest(
                "too_many_exports",
                format!("plugin declares more than {MAX_EXPORTS} exports"),
            ));
        }
        let mut export_names = std::collections::BTreeSet::new();
        for export in &plugin.exports {
            validate_dotted_name(&export.name, MAX_EXPORT_NAME_BYTES, "export name")?;
            if !export.name.contains('.') {
                return Err(PluginError::manifest(
                    "invalid_export_name",
                    format!(
                        "export `{}` must be namespaced (e.g. `solver.add`)",
                        export.name
                    ),
                ));
            }
            if let Some(prefix) = RESERVED_EXPORT_PREFIXES
                .iter()
                .find(|prefix| export.name.starts_with(**prefix))
            {
                return Err(PluginError::manifest(
                    "reserved_export_name",
                    format!("export `{}` uses reserved prefix `{prefix}`", export.name),
                ));
            }
            if !export_names.insert(export.name.clone()) {
                return Err(PluginError::manifest(
                    "duplicate_export",
                    format!("export `{}` is declared more than once", export.name),
                ));
            }
            validate_signature(&export.name, &export.signature)?;
        }
        if plugin.requires.len() > MAX_REQUIRES {
            return Err(PluginError::manifest(
                "too_many_capabilities",
                format!("plugin requires more than {MAX_REQUIRES} capabilities"),
            ));
        }
        let mut capabilities = std::collections::BTreeSet::new();
        for capability in &plugin.requires {
            if !KNOWN_CAPABILITIES.contains(&capability.as_str()) {
                return Err(PluginError::manifest(
                    "unknown_capability",
                    format!("unknown capability `{capability}`"),
                ));
            }
            if !capabilities.insert(capability.clone()) {
                return Err(PluginError::manifest(
                    "duplicate_capability",
                    format!("capability `{capability}` is declared more than once"),
                ));
            }
        }
        if plugin.artifacts.is_empty() {
            return Err(PluginError::manifest(
                "no_artifacts",
                "plugin must declare at least one artifact",
            ));
        }
        if plugin.artifacts.len() > MAX_ARTIFACTS {
            return Err(PluginError::manifest(
                "too_many_artifacts",
                format!("plugin declares more than {MAX_ARTIFACTS} artifacts"),
            ));
        }
        let mut targets = std::collections::BTreeSet::new();
        for artifact in &plugin.artifacts {
            let triple = artifact.target_triple()?;
            if !targets.insert(triple.clone()) {
                return Err(PluginError::manifest(
                    "duplicate_artifact_target",
                    format!("target `{triple}` is declared more than once"),
                ));
            }
            validate_artifact_file(&artifact.file)?;
            if !is_valid_sha256_hex(&artifact.sha256) {
                return Err(PluginError::manifest(
                    "invalid_sha256",
                    format!(
                        "artifact `{}` sha256 must be 64 hexadecimal characters",
                        artifact.file
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The parsed ABI requirement.
    pub fn abi_requirement(&self) -> Result<AbiRequirement, PluginError> {
        AbiRequirement::parse(&self.plugin.abi)
    }

    /// The parsed package version.
    pub fn package_version(&self) -> Result<PackageVersion, PluginError> {
        PackageVersion::parse(&self.plugin.version)
    }

    /// Resolve the artifact for `host`, requiring an exact target-triple match
    /// with matching pointer width.
    pub fn resolve_artifact<'a>(&'a self, host: &HostTarget) -> Result<&'a Artifact, PluginError> {
        for artifact in &self.plugin.artifacts {
            let triple = artifact.target_triple()?;
            if triple == *host.triple() {
                return Ok(artifact);
            }
        }
        Err(PluginError::artifact(
            "no_matching_artifact",
            format!(
                "plugin `{}` has no artifact for target `{}`",
                self.plugin.name,
                host.triple()
            ),
        )
        .with_detail(format!(
            "declared targets: {}",
            self.plugin
                .artifacts
                .iter()
                .map(|artifact| artifact.target.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    /// Resolve and verify the artifact for `host`, returning its path.
    ///
    /// Verification proves the file matches the manifest sha256; it does not
    /// grant trust or capability authorization.
    pub fn verify_artifact(
        &self,
        base_dir: &Path,
        host: &HostTarget,
    ) -> Result<(PathBuf, &Artifact), PluginError> {
        let artifact = self.resolve_artifact(host)?;
        let path = artifact.resolve_path(base_dir)?;
        verify_sha256_file(&path, &artifact.sha256)?;
        Ok((path, artifact))
    }
}

/// Semver-ish package version (major.minor.patch with optional pre-release and
/// build metadata). Manifest validation is deliberately not a full SemVer
/// range resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVersion {
    /// Original string from the manifest.
    pub raw: String,
    /// Major component.
    pub major: u64,
    /// Minor component.
    pub minor: u64,
    /// Patch component.
    pub patch: u64,
    /// Optional pre-release suffix (without the leading `-`).
    pub pre_release: Option<String>,
    /// Optional build metadata (without the leading `+`).
    pub build: Option<String>,
}

impl PackageVersion {
    /// Parse a semver-ish version string.
    pub fn parse(raw: &str) -> Result<Self, PluginError> {
        let invalid = |detail: String| {
            PluginError::manifest("invalid_version", format!("`{raw}` is not a valid version"))
                .with_detail(detail)
        };
        let (without_build, build) = match raw.split_once('+') {
            Some((left, right)) => {
                validate_identifiers(right).map_err(&invalid)?;
                (left, Some(right.to_string()))
            }
            None => (raw, None),
        };
        let (core, pre_release) = match without_build.split_once('-') {
            Some((left, right)) => {
                validate_identifiers(right).map_err(&invalid)?;
                (left, Some(right.to_string()))
            }
            None => (without_build, None),
        };
        let numbers: Vec<&str> = core.split('.').collect();
        if numbers.len() != 3 {
            return Err(invalid("expected `major.minor.patch`".to_string()));
        }
        let mut parsed = [0u64; 3];
        for (index, number) in numbers.iter().enumerate() {
            if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid(format!("component `{number}` is not numeric")));
            }
            if number.len() > 1 && number.starts_with('0') {
                return Err(invalid(format!("component `{number}` has a leading zero")));
            }
            parsed[index] = number
                .parse::<u64>()
                .map_err(|_| invalid(format!("component `{number}` is too large")))?;
        }
        Ok(Self {
            raw: raw.to_string(),
            major: parsed[0],
            minor: parsed[1],
            patch: parsed[2],
            pre_release,
            build,
        })
    }
}

fn validate_identifiers(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("empty version suffix".to_string());
    }
    for segment in value.split('.') {
        if segment.is_empty() {
            return Err("empty version suffix segment".to_string());
        }
        if !segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(format!("invalid version suffix `{segment}`"));
        }
    }
    Ok(())
}

/// A caret-style ABI requirement such as `^1.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiRequirement {
    /// Required major.
    pub major: u32,
    /// Required minimum minor.
    pub minor: u32,
    /// Original string from the manifest.
    pub raw: String,
}

impl AbiRequirement {
    /// Parse `^1`, `^1.0`, `1`, or `1.0`.
    pub fn parse(raw: &str) -> Result<Self, PluginError> {
        let trimmed = raw.trim();
        let numbers = trimmed.strip_prefix('^').unwrap_or(trimmed);
        let mut parts = numbers.split('.');
        let major = parts.next().unwrap_or_default();
        let minor = parts.next().unwrap_or("0");
        if parts.next().is_some() {
            return Err(PluginError::manifest(
                "invalid_abi_requirement",
                format!("`{raw}` is not a supported ABI requirement"),
            ));
        }
        let parse_component = |component: &str| -> Result<u32, PluginError> {
            if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(PluginError::manifest(
                    "invalid_abi_requirement",
                    format!("`{raw}` is not a supported ABI requirement"),
                ));
            }
            component.parse::<u32>().map_err(|_| {
                PluginError::manifest(
                    "invalid_abi_requirement",
                    format!("`{raw}` contains an out-of-range ABI component"),
                )
            })
        };
        Ok(Self {
            major: parse_component(major)?,
            minor: parse_component(minor)?,
            raw: raw.to_string(),
        })
    }

    /// Caret compatibility: `^M.m` means `>=M.m` and `<M+1.0`
    /// (with the SemVer `0.x` special case pinning the minor).
    pub fn is_compatible_with(&self, host: AbiVersion) -> bool {
        if self.major == 0 {
            host.major == 0 && host.minor == self.minor
        } else {
            host.major == self.major && host.minor >= self.minor
        }
    }
}

fn validate_dotted_name(name: &str, max_len: usize, what: &str) -> Result<(), PluginError> {
    if name.is_empty() {
        return Err(PluginError::manifest(
            "invalid_name",
            format!("{what} must not be empty"),
        ));
    }
    if name.len() > max_len {
        return Err(PluginError::manifest(
            "invalid_name",
            format!("{what} `{name}` is longer than {max_len} bytes"),
        ));
    }
    for segment in name.split('.') {
        if segment.is_empty() {
            return Err(PluginError::manifest(
                "invalid_name",
                format!("{what} `{name}` contains an empty segment"),
            ));
        }
        let mut bytes = segment.bytes();
        let first = bytes.next().unwrap_or(b'a');
        if !first.is_ascii_lowercase() {
            return Err(PluginError::manifest(
                "invalid_name",
                format!("{what} `{name}` must start each segment with a lowercase letter"),
            ));
        }
        if !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        }) {
            return Err(PluginError::manifest(
                "invalid_name",
                format!("{what} `{name}` may only contain lowercase letters, digits, `-` and `_`"),
            ));
        }
    }
    Ok(())
}

fn validate_signature(export: &str, signature: &str) -> Result<(), PluginError> {
    let trimmed = signature.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_SIGNATURE_WIRE_BYTES {
        return Err(PluginError::manifest(
            "invalid_signature",
            format!("export `{export}` has an empty or oversized signature"),
        ));
    }
    if !trimmed.starts_with('(') || !trimmed.contains("->") {
        return Err(PluginError::manifest(
            "invalid_signature",
            format!("export `{export}` signature must look like `(args) -> Return`"),
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(PluginError::manifest(
            "invalid_signature",
            format!("export `{export}` signature contains control characters"),
        ));
    }
    Ok(())
}

fn validate_artifact_file(file: &str) -> Result<(), PluginError> {
    if file.is_empty() || file.contains('\0') || file.len() > 256 {
        return Err(PluginError::manifest(
            "invalid_artifact_path",
            "artifact file must be a non-empty relative path of at most 256 bytes",
        ));
    }
    let path = Path::new(file);
    if path.is_absolute() {
        return Err(PluginError::manifest(
            "invalid_artifact_path",
            format!("artifact file `{file}` must be relative"),
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginError::manifest(
                    "invalid_artifact_path",
                    format!("artifact file `{file}` must not escape the package directory"),
                ));
            }
            Component::Normal(_) => {
                return Err(PluginError::manifest(
                    "invalid_artifact_path",
                    format!("artifact file `{file}` contains an empty component"),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = r#"
plugin:
  name: demo.solver
  version: 1.0.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: x86_64-unknown-linux-gnu
      file: solver.so
      sha256: "0000000000000000000000000000000000000000000000000000000000000000"
"#;

    fn valid_manifest() -> PluginManifest {
        let manifest = PluginManifest::parse_str(VALID_YAML).expect("parse");
        manifest.validate().expect("validate");
        manifest
    }

    #[test]
    fn valid_manifest_parses_and_validates() {
        let manifest = valid_manifest();
        assert_eq!(manifest.plugin.name, "demo.solver");
        assert_eq!(manifest.plugin.kind, PluginKind::Native);
        assert_eq!(manifest.abi_requirement().expect("abi").raw, "^1.0");
        assert_eq!(manifest.package_version().expect("version").patch, 0);
    }

    #[test]
    fn malformed_yaml_is_rejected() {
        let error = PluginManifest::parse_str("plugin: [unclosed").expect_err("must fail");
        assert_eq!(error.kind, crate::error::ErrorKind::Manifest);
        assert_eq!(error.code, "manifest_parse_failed");
        assert!(PluginManifest::parse_str("not_a_plugin: 1").is_err());
    }

    #[test]
    fn invalid_fields_are_rejected() {
        let cases = [
            ("name", "DemoSolver"),
            ("version", "1.0"),
            ("abi", "^2.0"),
            ("kind", "inprocess"),
            ("file", "../solver.so"),
            ("sha256", "xyz"),
        ];
        for (field, value) in cases {
            let yaml = match field {
                "name" => VALID_YAML.replace("name: demo.solver", &format!("name: {value}")),
                "version" => VALID_YAML.replace("version: 1.0.0", &format!("version: {value}")),
                "abi" => VALID_YAML.replace("abi: \"^1.0\"", &format!("abi: \"{value}\"")),
                "kind" => VALID_YAML.replace("kind: native", &format!("kind: {value}")),
                "file" => VALID_YAML.replace("file: solver.so", &format!("file: {value}")),
                "sha256" => VALID_YAML.replace(
                    "sha256: \"0000000000000000000000000000000000000000000000000000000000000000\"",
                    &format!("sha256: \"{value}\""),
                ),
                _ => unreachable!(),
            };
            match PluginManifest::parse_str(&yaml) {
                Ok(manifest) => assert!(
                    manifest.validate().is_err(),
                    "`{field}` = `{value}` should fail validation"
                ),
                Err(error) => assert_eq!(
                    error.kind,
                    crate::error::ErrorKind::Manifest,
                    "`{field}` = `{value}` should fail manifest handling"
                ),
            }
        }
    }

    #[test]
    fn duplicate_and_reserved_exports_are_rejected() {
        let duplicate = VALID_YAML.replace(
            "    - name: solver.add\n      signature: \"($a: Int, $b: Int) -> Int\"",
            "    - name: solver.add\n      signature: \"($a: Int) -> Int\"\n    - name: solver.add\n      signature: \"($a: Int) -> Int\"",
        );
        let manifest = PluginManifest::parse_str(&duplicate).expect("parse");
        assert_eq!(
            manifest.validate().expect_err("duplicate").code,
            "duplicate_export"
        );

        let reserved = VALID_YAML.replace("solver.add", "builtin.print");
        let manifest = PluginManifest::parse_str(&reserved).expect("parse");
        assert_eq!(
            manifest.validate().expect_err("reserved").code,
            "reserved_export_name"
        );
    }

    #[test]
    fn abi_requirement_caret_semantics() {
        let v1 = AbiVersion::new(1, 0);
        assert!(AbiRequirement::parse("^1.0")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(AbiRequirement::parse("^1")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(AbiRequirement::parse("1.0")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(!AbiRequirement::parse("^1.1")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(!AbiRequirement::parse("^2.0")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(!AbiRequirement::parse("^0.1")
            .expect("parse")
            .is_compatible_with(v1));
        assert!(AbiRequirement::parse("nonsense").is_err());
    }

    #[test]
    fn artifact_resolution_requires_exact_host_target() {
        let manifest = valid_manifest();
        let host = HostTarget::current();
        if host.triple().as_str() == "x86_64-unknown-linux-gnu" {
            let artifact = manifest.resolve_artifact(&host).expect("resolve");
            assert_eq!(artifact.file, "solver.so");
        } else {
            assert_eq!(
                manifest
                    .resolve_artifact(&host)
                    .expect_err("other target")
                    .code,
                "no_matching_artifact"
            );
        }
    }
}
