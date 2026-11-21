//! Strict, independent expression and statement parser for executable Markdown
//! payloads.
//!
//! This crate is the only grammar for Marqdo executable payloads. It knows
//! nothing about Markdown and nothing about the runtime: the Markdown AST
//! decides *where* a payload exists, this crate decides *what the payload
//! means* (see `spec/expression.ebnf` and `spec/evaluation.md`).
//!
//! # Public API
//!
//! ```rust
//! use marqdo_expression::{parse_expression, parse_statement};
//!
//! let stmt = parse_statement("结果 = 阶乘(n=3)").unwrap();
//! assert_eq!(stmt.span().start, 0);
//!
//! let expr = parse_expression("2 ** -1").unwrap();
//! assert_eq!(expr.span().start, 0);
//! ```
//!
//! The contractual surface is:
//!
//! * [`parse_expression`] — parse a complete expression payload.
//! * [`parse_statement`] — parse a complete statement payload (assignment or
//!   expression).
//! * [`parse_arguments`] — parse a leading `( ... )` argument list, used for
//!   Markdown-link calls such as `> [计算面积](./math.mq.md#计算面积)(r=3)`.
//! * the [`ast`] module (re-exported at the crate root).
//! * [`ParseError`] — structured `expr.parse` diagnostics with byte spans.
//!
//! Everything else, including [`lexer`], is an implementation detail that is
//! exposed only to make diagnostics and tests easier.
//!
//! # Grammar summary
//!
//! The grammar is frozen in `spec/expression.ebnf`. In order of decreasing
//! precedence: prefix `not` / `-` / `+`, `**` (right associative), postfix
//! calls `.name` / `[index]`, `* / // %`, `+ -`, a single non-associative
//! comparison, `and`, `or`. Assignment is only available at statement start;
//! its target is a bare identifier or an index expression (`m[8] = 42`).
//!
//! Notable lexical rules:
//!
//! * identifiers use Unicode XID (`XID_Start XID_Continue*`), so `阶乘` and
//!   `café` are identifiers;
//! * `true`, `false`, `None`, `and`, `or`, `not` are reserved keywords;
//! * integers are arbitrary-precision decimal literals;
//! * floats require digits on both sides of `.` (`.5` and `1.` are invalid)
//!   and may carry an exponent;
//! * text literals are double quoted and support `\\`, `\"`, `\n`, `\t`, `\r`
//!   and `\u{...}` escapes;
//! * comments are not supported;
//! * nesting deeper than [`MAX_NESTING_DEPTH`] is a structured parse error, so
//!   hostile payloads cannot overflow the parser stack.
//!
//! # Spans and errors
//!
//! Spans are half-open byte offsets relative to the payload string. Errors use
//! the stable diagnostic code [`EXPR_PARSE`] (`"expr.parse"`) and follow the
//! same shape used elsewhere in the toolchain (code + message + span).
//!
//! # Example payloads
//!
//! ```rust
//! use marqdo_expression::parse_statement;
//!
//! let stmt = parse_statement("打印(\"面积 = \" + 文本(area))").unwrap();
//! assert!(matches!(
//!     stmt.kind,
//!     marqdo_expression::StatementKind::Expr(_)
//! ));
//! ```
//!
//! The parser never panics on malformed input: every failure is a
//! [`ParseError`].

#![forbid(unsafe_code)]

pub mod ast;
pub mod lexer;
mod parser;
pub mod statement;

use std::fmt;

use serde::Serialize;

pub use ast::{
    Arg, AssignTarget, BinaryOp, Expr, Ident, RecordEntry, Statement, StatementKind, UnaryOp,
};
pub use marqdo_span::Span;
pub use parser::MAX_NESTING_DEPTH;
pub use statement::{parse_payload_statement, PayloadStatement};

/// Stable diagnostic code for every expression parse failure.
pub const EXPR_PARSE: &str = "expr.parse";

/// A structured expression/statement parse failure.
///
/// `code` is always [`EXPR_PARSE`]. `span` is a byte range into the payload
/// string and may be empty (for example at end of input).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseError {
    /// Stable diagnostic code, always `"expr.parse"`.
    pub code: &'static str,
    /// Human readable explanation of the failure.
    pub message: String,
    /// Byte range in the payload where the failure was detected.
    pub span: Span,
}

impl ParseError {
    /// Create a parse error with the fixed `expr.parse` code.
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            code: EXPR_PARSE,
            message: message.into(),
            span,
        }
    }

    /// Stable diagnostic code (always `"expr.parse"`).
    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} at {}", self.code, self.message, self.span)
    }
}

impl std::error::Error for ParseError {}

/// Parse a complete expression payload.
///
/// The whole input must be consumed; empty input, trailing tokens, unsupported
/// constructs and assignments are [`ParseError`]s.
pub fn parse_expression(source: &str) -> Result<Expr, ParseError> {
    parser::parse_expression(source)
}

/// Parse a complete statement payload.
///
/// A statement is either `<bare identifier> = <expression>` or a bare
/// expression (typically a call). The whole input must be consumed.
pub fn parse_statement(source: &str) -> Result<Statement, ParseError> {
    parser::parse_statement(source)
}

/// Parse a leading parenthesised argument list.
///
/// `source` must start with `(` and contain exactly one argument list, e.g.
/// `(r=3, precision=2)`. Argument rules are identical to call arguments:
/// positional arguments precede named arguments, named arguments are unique,
/// and a single trailing comma is allowed. Trailing tokens are an error.
///
/// This exists so the frontend can parse Markdown-link calls of the form
/// `> [计算面积](./math.mq.md#计算面积)(r=3)` without re-lexing the payload.
pub fn parse_arguments(source: &str) -> Result<Vec<Arg>, ParseError> {
    parser::parse_arguments(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_api_parses_expression_and_statement() {
        assert!(parse_expression("1 + 2 * 3").is_ok());
        assert!(parse_statement("x = 1").is_ok());
        assert!(parse_arguments("(r=3)").is_ok());
    }

    #[test]
    fn empty_payloads_are_errors() {
        for source in ["", "   ", "\n\t"] {
            let err = parse_expression(source).unwrap_err();
            assert_eq!(err.code, EXPR_PARSE);
            assert_eq!(err.code(), "expr.parse");
            assert!(err.message.contains("expression"));

            let err = parse_statement(source).unwrap_err();
            assert_eq!(err.code, EXPR_PARSE);
            assert!(err.message.contains("statement"));
        }
    }

    #[test]
    fn parse_error_displays_code_and_span_and_implements_error() {
        let err = parse_expression("1 2").unwrap_err();
        let rendered = err.to_string();
        assert!(rendered.starts_with("expr.parse: "), "{rendered}");
        assert!(rendered.contains("2..3"), "{rendered}");
        assert!(rendered.contains("unexpected"), "{rendered}");

        let dyn_error: &dyn std::error::Error = &err;
        assert!(dyn_error.source().is_none());

        let custom = ParseError::new(Span::new(4, 9), "boom");
        assert_eq!(custom.code, "expr.parse");
        assert_eq!(custom.message, "boom");
        assert_eq!(custom.span, Span::new(4, 9));
    }

    #[test]
    fn ast_types_are_reachable_from_the_crate_root() {
        let expr: Expr = parse_expression("f(1)").unwrap();
        let statement: Statement = parse_statement("f(1)").unwrap();
        let _: StatementKind = statement.kind;
        let _: BinaryOp = BinaryOp::Add;
        let _: UnaryOp = UnaryOp::Not;
        let _: Arg = Arg {
            name: None,
            value: expr,
            span: Span::default(),
        };
        let _: RecordEntry = RecordEntry {
            key: "k".to_owned(),
            key_span: Span::default(),
            value: Expr::None,
        };
        let _: Ident = Ident {
            name: "x".to_owned(),
            span: Span::default(),
        };
    }
}
