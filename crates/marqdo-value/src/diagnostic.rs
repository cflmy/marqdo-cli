//! Machine readable diagnostics shared by `check`, `inspect` and `run`.

use marqdo_span::Span;
use serde::{Deserialize, Serialize};

/// Stable diagnostic codes. CI may depend on these strings; they must not be
/// renamed without an ADR because they are part of the CLI contract.
pub mod codes {
    /// A Markdown AST node occupies an executable position but has an invalid shape.
    pub const MARKDOWN_INVALID_CONSTRUCT: &str = "markdown.invalid_construct";
    /// A node that would be executable appears outside any function body and is
    /// therefore inert document content.
    pub const MARKDOWN_INERT_EXECUTABLE: &str = "markdown.inert_executable";
    /// Front matter is malformed or appears somewhere other than the document start.
    pub const FRONTMATTER_INVALID: &str = "frontmatter.invalid";
    /// A heading uses an invalid `@exec`/`@knowledge` mode marker.
    pub const SEM_INVALID_MODE_MARKER: &str = "sem.invalid_mode_marker";
    /// A mode declaration has no semantic heading name.
    pub const SEM_EMPTY_UNIT_NAME: &str = "sem.empty_unit_name";
    /// Duplicate canonical heading/unit name in one scope.
    pub const SEM_DUPLICATE_UNIT_NAME: &str = "sem.duplicate_unit_name";
    /// Executable payload appears under no Exec H2 function.
    pub const SEM_EXEC_WITHOUT_FUNCTION: &str = "sem.exec_without_function";
    /// A payload carrier contains a nested inline structure.
    pub const SEM_PAYLOAD_NESTED: &str = "sem.payload_nested";
    /// A payload carrier is empty in an Exec scope.
    pub const SEM_EMPTY_PAYLOAD: &str = "sem.empty_payload";
    /// A payload failed semantic statement validation.
    pub const SEM_SYNTAX: &str = "sem.syntax";
    /// `else` does not bind an open `if`.
    pub const SEM_UNMATCHED_ELSE: &str = "sem.unmatched_else";
    /// A second `else` appears for one `if`.
    pub const SEM_DUPLICATE_ELSE: &str = "sem.duplicate_else";
    /// `end` closes no open frame.
    pub const SEM_UNMATCHED_END: &str = "sem.unmatched_end";
    /// A control frame is not closed before the heading ends.
    pub const SEM_UNCLOSED_CONTROL: &str = "sem.unclosed_control";
    /// `params(...)` is not the first direct Exec H2 payload.
    pub const SEM_SIGNATURE_POSITION: &str = "sem.signature_position";
    /// A version directive such as `marqdo_semantics` is forbidden.
    pub const SEM_UNSUPPORTED_VERSION_DIRECTIVE: &str = "sem.unsupported_version_directive";
    /// `run` found no callable entry function.
    pub const SEM_NO_ENTRY_FUNCTION: &str = "sem.no_entry_function";
    /// A signature was inferred rather than declared.
    pub const SEM_INFERRED_SIGNATURE: &str = "sem.inferred_signature";
    /// An executable payload failed to parse as an expression or statement.
    pub const EXPR_PARSE: &str = "expr.parse";
    /// A function name is referenced but no such function exists.
    pub const SYMBOL_UNKNOWN: &str = "symbol.unknown";
    /// A variable is read before any binding is guaranteed to exist.
    pub const SYMBOL_UNINITIALIZED: &str = "symbol.uninitialized";
    /// A call supplies the wrong number of arguments.
    pub const CALL_ARITY: &str = "call.arity";
    /// A call supplies an argument of the wrong type.
    pub const CALL_TYPE: &str = "call.type";
    /// An imported module cannot be resolved or loaded.
    pub const MODULE_UNRESOLVED: &str = "module.unresolved";
    /// A module import cycle was detected.
    pub const MODULE_CYCLE: &str = "module.cycle";
    /// A Markdown reference or link target cannot be resolved.
    pub const REF_UNRESOLVED: &str = "ref.unresolved";
    /// The requested math operation is outside the supported subset.
    pub const MATH_UNSUPPORTED: &str = "math.unsupported";
    /// A foreign language kernel is not available on this machine.
    pub const KERNEL_UNAVAILABLE: &str = "kernel.unavailable";
    /// A capability check rejected the requested operation.
    pub const PERMISSION_DENIED: &str = "permission.denied";
    /// A plugin artifact does not match the supported ABI version.
    pub const PLUGIN_ABI_MISMATCH: &str = "plugin.abi_mismatch";
    /// A resource budget (fuel, stack, memory, time) was exhausted.
    pub const RESOURCE_EXHAUSTED: &str = "resource.exhausted";
    /// A runtime type error that is not better described by a more specific code.
    pub const RUNTIME_TYPE: &str = "runtime.type";
    /// A runtime index or key error.
    pub const RUNTIME_INDEX: &str = "runtime.index";
    /// Invalid project manifest.
    pub const PROJECT_MANIFEST: &str = "project.manifest";
    /// Invalid project lockfile.
    pub const PROJECT_LOCK: &str = "project.lock";
    /// Dependency resolution failed.
    pub const PROJECT_DEPENDENCY: &str = "project.dependency";
    /// Internal invariant violation; never expected in normal operation.
    pub const INTERNAL: &str = "internal.error";
}

/// Severity level of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The input is invalid and execution/checking cannot proceed.
    Error,
    /// The input is valid but suspicious or potentially non-portable.
    Warning,
    /// Informational note attached to another diagnostic.
    Info,
}

impl Severity {
    /// Lowercase name used in JSON output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Secondary source location attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Related {
    /// Optional span of the related location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Why this location is relevant.
    pub message: String,
}

/// A structured, machine readable diagnostic.
///
/// The JSON shape is part of the CLI contract:
/// `{ code, severity, span, message, related, hints }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable code from [`codes`].
    pub code: String,
    /// Severity of the diagnostic.
    pub severity: Severity,
    /// Source span (byte offsets into the file), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Human readable message. Not part of the compatibility contract.
    pub message: String,
    /// Related locations, e.g. a conflicting declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Related>,
    /// Suggested fixes or authoring hints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl Diagnostic {
    /// Create an error diagnostic.
    pub fn error(code: impl Into<String>, span: Option<Span>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            span,
            message: message.into(),
            related: Vec::new(),
            hints: Vec::new(),
        }
    }

    /// Create a warning diagnostic.
    pub fn warning(
        code: impl Into<String>,
        span: Option<Span>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(code, span, message)
        }
    }

    /// Add an authoring hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }

    /// Add a related source location.
    pub fn with_related(mut self, span: Option<Span>, message: impl Into<String>) -> Self {
        self.related.push(Related {
            span,
            message: message.into(),
        });
        self
    }

    /// Add a related span that is a suitable fix location.
    pub fn with_help_span(self, span: Span, message: impl Into<String>) -> Self {
        self.with_related(Some(span), message)
    }

    /// Whether this diagnostic is fatal.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_json_shape_is_stable() {
        let diag = Diagnostic::error(codes::EXPR_PARSE, Some(Span::new(3, 9)), "bad expression")
            .with_hint("use a code span")
            .with_related(None, "declared here");
        let json = serde_json::to_value(&diag).unwrap();
        assert_eq!(json["code"], "expr.parse");
        assert_eq!(json["severity"], "error");
        assert_eq!(json["span"]["start"], 3);
        assert_eq!(json["hints"][0], "use a code span");
        assert_eq!(json["related"][0]["message"], "declared here");
        // Absent optional fields are omitted rather than null.
        let minimal = Diagnostic::error(codes::INTERNAL, None, "oops");
        let json = serde_json::to_value(&minimal).unwrap();
        assert!(json.get("span").is_none());
        assert!(json.get("related").is_none());
    }
}
