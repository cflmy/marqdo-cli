//! Structured runtime errors with source spans and call frames.

use crate::codes;
use crate::diagnostic::Diagnostic;
use marqdo_span::Span;

/// Category of a runtime failure. Each category maps to a stable diagnostic
/// code so that CI and agents can branch on codes instead of message wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// A value had the wrong type for the operation.
    Type,
    /// A call supplied the wrong number of arguments.
    Arity,
    /// A name is not bound in any reachable scope.
    UnknownSymbol,
    /// A name is bound but may be read before assignment.
    Uninitialized,
    /// Division or modulo by zero.
    DivisionByZero,
    /// A value was used as a function but is not callable.
    NotCallable,
    /// A map key or record field does not exist.
    UnknownField,
    /// A list/map index is out of bounds.
    IndexOutOfBounds,
    /// A resource budget was exhausted (fuel, stack, output size).
    ResourceExhausted,
    /// A capability check denied the operation.
    PermissionDenied,
    /// A foreign language kernel is unavailable.
    KernelUnavailable,
    /// The requested math operation is unsupported.
    MathUnsupported,
    /// Plugin ABI negotiation failed.
    PluginAbi,
    /// A resource reference matches more than one resource (S-07: never pick
    /// the first silently).
    RefAmbiguous,
    /// A plugin reported an execution failure.
    PluginFailure,
    /// A foreign kernel failed (non-zero exit, protocol violation).
    ForeignFailure,
    /// A host operation failed for an environmental reason.
    HostError,
    /// Internal invariant violation.
    Internal,
}

impl ErrorCategory {
    /// Stable diagnostic code for this category.
    pub const fn code(self) -> &'static str {
        match self {
            ErrorCategory::Type => codes::RUNTIME_TYPE,
            ErrorCategory::Arity => codes::CALL_ARITY,
            ErrorCategory::UnknownSymbol => codes::SYMBOL_UNKNOWN,
            ErrorCategory::Uninitialized => codes::SYMBOL_UNINITIALIZED,
            ErrorCategory::DivisionByZero => "runtime.division_by_zero",
            ErrorCategory::NotCallable => "call.not_callable",
            ErrorCategory::UnknownField => "runtime.unknown_field",
            ErrorCategory::IndexOutOfBounds => codes::RUNTIME_INDEX,
            ErrorCategory::ResourceExhausted => codes::RESOURCE_EXHAUSTED,
            ErrorCategory::PermissionDenied => codes::PERMISSION_DENIED,
            ErrorCategory::KernelUnavailable => codes::KERNEL_UNAVAILABLE,
            ErrorCategory::MathUnsupported => codes::MATH_UNSUPPORTED,
            ErrorCategory::PluginAbi => codes::PLUGIN_ABI_MISMATCH,
            ErrorCategory::RefAmbiguous => "ref.ambiguous",
            ErrorCategory::PluginFailure => "plugin.failure",
            ErrorCategory::ForeignFailure => "foreign.failure",
            ErrorCategory::HostError => "host.error",
            ErrorCategory::Internal => codes::INTERNAL,
        }
    }
}

/// One entry of the Marqdo call stack.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallFrame {
    /// Canonical function name (or `<main>` for the entry function).
    pub function: String,
    /// Source location of the call site in the caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// A structured runtime failure.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeError {
    /// Failure category.
    pub category: ErrorCategory,
    /// Location in the source that caused the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Human readable message.
    pub message: String,
    /// Call frames from innermost to outermost.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<CallFrame>,
    /// Underlying cause (e.g. a host or foreign kernel error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<RuntimeError>>,
    /// Effect trace recorded by the host, when available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effect_trace: Vec<String>,
}

impl RuntimeError {
    /// Create a new runtime error.
    pub fn new(category: ErrorCategory, span: Option<Span>, message: impl Into<String>) -> Self {
        Self {
            category,
            span,
            message: message.into(),
            frames: Vec::new(),
            cause: None,
            effect_trace: Vec::new(),
        }
    }

    /// Convenience constructor for type errors.
    pub fn type_error(span: Option<Span>, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Type, span, message)
    }

    /// Record a call frame (called while unwinding).
    pub fn push_frame(&mut self, function: impl Into<String>, span: Option<Span>) {
        self.frames.push(CallFrame {
            function: function.into(),
            span,
        });
    }

    /// Add a cause.
    pub fn with_cause(mut self, cause: RuntimeError) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    /// The stable diagnostic code for this failure.
    pub fn code(&self) -> &'static str {
        self.category.code()
    }

    /// Render as a user facing diagnostic.
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut diag = Diagnostic::error(self.code(), self.span, self.message.clone());
        for frame in &self.frames {
            diag = diag.with_related(frame.span, format!("in function `{}`", frame.function));
        }
        if let Some(cause) = &self.cause {
            diag = diag.with_related(cause.span, format!("caused by: {}", cause.message));
        }
        diag
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message)
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_rendered_as_related_diagnostics() {
        let mut err = RuntimeError::type_error(Some(Span::new(1, 2)), "expected Bool");
        err.push_frame("阶乘", None);
        let diag = err.to_diagnostic();
        assert_eq!(diag.code, "runtime.type");
        assert_eq!(diag.related.len(), 1);
        assert_eq!(diag.related[0].message, "in function `阶乘`");
    }
}
