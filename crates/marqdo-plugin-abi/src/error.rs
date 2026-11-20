//! Structured plugin errors shared by host-side loading, manifest validation,
//! artifact verification, wire decoding and invocation.
//!
//! Errors are deliberately data, not opaque strings: the host CLI renders
//! them for humans, the wire protocol ships them to workers/plugins, and
//! tests assert on the stable machine-readable `code`.

use serde::{Deserialize, Serialize};

/// Stable error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// `plugin.yaml` could not be parsed or is semantically invalid.
    Manifest,
    /// ABI major/minor negotiation or ABI table mismatch.
    Abi,
    /// Artifact resolution, target triples and sha256 verification.
    Artifact,
    /// Dynamic library loading or entry-point resolution.
    Load,
    /// `wire_version` 1 JSON envelope is malformed or over the size limit.
    Wire,
    /// A requested host capability was not granted or failed.
    Capability,
    /// A plugin invocation failed or returned an invalid result.
    Invoke,
    /// A valid-but-unsupported feature (plugin kind, ABI requirement, ...).
    Unsupported,
}

impl ErrorKind {
    /// Stable snake_case name used on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Manifest => "manifest",
            ErrorKind::Abi => "abi",
            ErrorKind::Artifact => "artifact",
            ErrorKind::Load => "load",
            ErrorKind::Wire => "wire",
            ErrorKind::Capability => "capability",
            ErrorKind::Invoke => "invoke",
            ErrorKind::Unsupported => "unsupported",
        }
    }
}

impl ErrorKind {
    /// Parse a stable wire name back into an error category.
    pub fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "manifest" => Some(ErrorKind::Manifest),
            "abi" => Some(ErrorKind::Abi),
            "artifact" => Some(ErrorKind::Artifact),
            "load" => Some(ErrorKind::Load),
            "wire" => Some(ErrorKind::Wire),
            "capability" => Some(ErrorKind::Capability),
            "invoke" => Some(ErrorKind::Invoke),
            "unsupported" => Some(ErrorKind::Unsupported),
            _ => None,
        }
    }
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured, serializable plugin error.
///
/// `kind` is the category, `code` is a stable machine-readable identifier
/// (e.g. `abi_major_mismatch`), `message` is human-readable and must never
/// contain secrets, and `detail` is optional extra context (paths, ABI
/// numbers, ...) that is equally safe to log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("[{kind}] {code}: {message}")]
pub struct PluginError {
    /// Error category.
    pub kind: ErrorKind,
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable summary (never contains secrets).
    pub message: String,
    /// Optional non-sensitive detail for logs/diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Whether the caller may retry the operation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub recoverable: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl PluginError {
    /// Create an error with no detail.
    pub fn new(kind: ErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
            detail: None,
            recoverable: false,
        }
    }

    /// Attach non-sensitive detail.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Mark the operation as retryable.
    #[must_use]
    pub fn recoverable(mut self) -> Self {
        self.recoverable = true;
        self
    }

    /// A manifest parse/validation error.
    pub fn manifest(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Manifest, code, message)
    }

    /// An ABI negotiation/table error.
    pub fn abi(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Abi, code, message)
    }

    /// An artifact resolution/hash error.
    pub fn artifact(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Artifact, code, message)
    }

    /// A dynamic library/entry-point load error.
    pub fn load(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Load, code, message)
    }

    /// A wire envelope error.
    pub fn wire(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Wire, code, message)
    }

    /// A capability error.
    pub fn capability(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Capability, code, message)
    }

    /// An invocation error.
    pub fn invoke(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Invoke, code, message)
    }

    /// An unsupported-feature error.
    pub fn unsupported(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, code, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_serializable_roundtrip() {
        let error = PluginError::abi("abi_major_mismatch", "host is ABI 1")
            .with_detail("plugin reported ABI 2.0");
        let json = serde_json::to_string(&error).expect("serialize");
        assert!(json.contains("\"kind\":\"abi\""));
        assert!(json.contains("\"code\":\"abi_major_mismatch\""));
        let back: PluginError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, error);
    }

    #[test]
    fn optional_fields_are_omitted() {
        let error = PluginError::manifest("bad_manifest", "missing plugin block");
        let json = serde_json::to_string(&error).expect("serialize");
        assert!(!json.contains("detail"));
        assert!(!json.contains("recoverable"));
    }
}
