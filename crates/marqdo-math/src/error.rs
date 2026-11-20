//! Structured errors returned by the native math kernel.
//!
//! The kernel never panics on untrusted LaTeX. Every failure is reported as a
//! [`MathError`] with a stable [`MathErrorKind`], a human readable message and
//! (for parse failures) the byte offset into the original LaTeX source. The
//! host layer is expected to map [`MathErrorKind::Unsupported`] to the
//! `math.unsupported` diagnostic code and the remaining kinds to the matching
//! expression diagnostics.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Broad category of a math kernel failure.
///
/// The enum is deliberately small: the kernel only distinguishes "the input is
/// not in the supported grammar", "the input is in the grammar but outside the
/// supported subset", "a name is unbound" and "a value has the wrong type".
/// Resource-bound violations (exponent/result size) are reported as
/// [`MathErrorKind::Unsupported`] because they are a documented limitation of
/// the subset rather than a parse or type problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathErrorKind {
    /// The input is malformed LaTeX for the supported subset.
    Parse,
    /// The input is well-formed but outside the deliberately limited subset, or
    /// the requested operation/equation shape is not supported.
    Unsupported,
    /// A variable used by the formula has no binding.
    UndefinedVariable,
    /// A binding or operand has an incompatible type, or an operation is
    /// undefined for the supplied values (for example division by zero).
    Type,
}

impl MathErrorKind {
    /// Stable lowercase label used by diagnostics and tests.
    pub const fn as_str(self) -> &'static str {
        match self {
            MathErrorKind::Parse => "parse",
            MathErrorKind::Unsupported => "unsupported",
            MathErrorKind::UndefinedVariable => "undefined-variable",
            MathErrorKind::Type => "type",
        }
    }
}

impl fmt::Display for MathErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structured error produced by [`crate::parse`], [`crate::evaluate`] or
/// [`crate::substitute`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MathError {
    /// Failure category.
    pub kind: MathErrorKind,
    /// Human readable explanation. Messages are diagnostic text; callers should
    /// branch on [`MathError::kind`] rather than on the wording.
    pub message: String,
    /// Byte offset into the original LaTeX source, when the failure is tied to
    /// a specific token.
    pub position: Option<usize>,
}

impl MathError {
    /// Construct an error with an explicit kind and optional byte position.
    pub fn new(kind: MathErrorKind, message: impl Into<String>, position: Option<usize>) -> Self {
        Self {
            kind,
            message: message.into(),
            position,
        }
    }

    /// Construct a [`MathErrorKind::Parse`] error.
    pub fn parse(message: impl Into<String>, position: Option<usize>) -> Self {
        Self::new(MathErrorKind::Parse, message, position)
    }

    /// Construct a [`MathErrorKind::Unsupported`] error.
    pub fn unsupported(message: impl Into<String>, position: Option<usize>) -> Self {
        Self::new(MathErrorKind::Unsupported, message, position)
    }

    /// Construct a [`MathErrorKind::UndefinedVariable`] error.
    pub fn undefined_variable(message: impl Into<String>, position: Option<usize>) -> Self {
        Self::new(MathErrorKind::UndefinedVariable, message, position)
    }

    /// Construct a [`MathErrorKind::Type`] error.
    pub fn type_error(message: impl Into<String>, position: Option<usize>) -> Self {
        Self::new(MathErrorKind::Type, message, position)
    }

    /// The documented result-size guard was hit. Reported as `Unsupported` so
    /// callers can surface a stable `math.unsupported` diagnostic instead of a
    /// crash or an out-of-memory abort.
    pub fn size_bound(message: impl Into<String>) -> Self {
        Self::unsupported(message, None)
    }
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(position) = self.position {
            write!(f, " at byte {position}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for MathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_kind_and_position() {
        let err = MathError::parse("expected an expression", Some(3));
        assert_eq!(err.to_string(), "parse at byte 3: expected an expression");
        assert_eq!(err.kind.as_str(), "parse");
    }

    #[test]
    fn constructors_use_stable_kinds() {
        assert_eq!(
            MathError::unsupported("x", None).kind,
            MathErrorKind::Unsupported
        );
        assert_eq!(
            MathError::undefined_variable("x", None).kind,
            MathErrorKind::UndefinedVariable
        );
        assert_eq!(MathError::type_error("x", None).kind, MathErrorKind::Type);
        assert_eq!(MathError::size_bound("x").kind, MathErrorKind::Unsupported);
        assert_eq!(
            MathErrorKind::UndefinedVariable.to_string(),
            "undefined-variable"
        );
    }
}
