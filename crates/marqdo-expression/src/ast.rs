//! Abstract syntax tree for the Marqdo expression grammar v1.
//!
//! All spans are byte offsets relative to the payload string that was handed to
//! [`crate::parse_expression`], [`crate::parse_statement`] or
//! [`crate::parse_arguments`]. The frontend re-bases them onto the Markdown
//! source through the text/code segments that made up the payload.
//!
//! # Span coverage
//!
//! Every node that carries a stored span exposes it through [`Expr::span`] /
//! [`Statement::span`]. The frozen v1 AST shape stores a span only on the
//! structural variants (`Unary`, `Binary`, `Call`, `Member`, `Index`), on
//! [`Ident`], [`Arg`] and [`RecordEntry`]. Scalar and container literals
//! (`Int`, `Float`, `Text`, `Bool`, `None`, `List`, `Record`) intentionally
//! have no span field: [`Expr::span`] returns the empty span `0..0` for them.
//! Consumers that need a location for a literal should use the nearest
//! enclosing structural node or the enclosing [`Statement::span`].

use marqdo_span::Span;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

/// A bare identifier together with its source span.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident {
    /// Identifier text as written (Unicode XID, decoded verbatim).
    pub name: String,
    /// Span of the identifier token.
    pub span: Span,
}

/// One argument of a call, optionally named.
///
/// ```text
/// f(1, r=3)
///   ^    ^ named argument, `name == Some("r")`
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arg {
    /// `Some` when the argument was written as `name = value`.
    pub name: Option<String>,
    /// Argument value expression.
    pub value: Expr,
    /// Span of the whole argument (name through value).
    pub span: Span,
}

/// One key/value entry of a record literal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordEntry {
    /// Entry key (identifier text or decoded text-literal contents).
    pub key: String,
    /// Span of the key token (including the quotes for a text-literal key).
    pub key_span: Span,
    /// Entry value expression.
    pub value: Expr,
}

/// Binary operators, listed from loosest to tightest binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOp {
    /// Short-circuiting logical or (`or`).
    Or,
    /// Short-circuiting logical and (`and`).
    And,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/` (always yields `Float` at runtime).
    Div,
    /// `//` (floor division, `Int` only).
    FloorDiv,
    /// `%` (remainder, `Int` only).
    Mod,
    /// `**` (right associative).
    Pow,
}

/// Prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOp {
    /// `not`
    Not,
    /// unary `-`
    Neg,
    /// unary `+`
    Pos,
}

/// An expression node.
///
/// The variant set and payloads are the frozen public contract of this crate.
/// See the module docs for the span policy of literal variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// Arbitrary-precision decimal integer literal.
    Int(BigInt),
    /// IEEE-754 binary64 float literal.
    Float(f64),
    /// Decoded text literal (escapes already resolved).
    Text(String),
    /// `true` / `false`.
    Bool(bool),
    /// `None`.
    None,
    /// A variable, parameter or function name reference.
    Ident(Ident),
    /// `[a, b, c]`.
    List(Vec<Expr>),
    /// `{key: value}`.
    Record(Vec<RecordEntry>),
    /// Prefix operator application.
    Unary {
        /// Applied prefix operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<Expr>,
        /// Span from the operator through the end of the operand.
        span: Span,
    },
    /// Infix operator application.
    Binary {
        /// Applied infix operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
        /// Span of the whole infix expression.
        span: Span,
    },
    /// Call expression.
    Call {
        /// Callee expression.
        callee: Box<Expr>,
        /// Arguments in source order.
        args: Vec<Arg>,
        /// Span from the callee start through the closing `)`.
        span: Span,
    },
    /// Member access (`base.name`).
    Member {
        /// Object being projected.
        base: Box<Expr>,
        /// Member name (never a keyword).
        name: String,
        /// Span from the base start through the member name.
        span: Span,
    },
    /// Index access (`base[index]`).
    Index {
        /// Object being indexed.
        base: Box<Expr>,
        /// Index expression.
        index: Box<Expr>,
        /// Span from the base start through the closing `]`.
        span: Span,
    },
}

impl Expr {
    /// Best known source span of this expression.
    ///
    /// Returns the empty span `0..0` for variants that do not store a span
    /// (`Int`, `Float`, `Text`, `Bool`, `None`, `List`, `Record`); see the
    /// module documentation for the rationale.
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident(ident) => ident.span,
            Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Member { span, .. }
            | Expr::Index { span, .. } => *span,
            Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => {
                Span::default()
            }
            Expr::List(_) | Expr::Record(_) => Span::default(),
        }
    }
}

/// The payload of a statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StatementKind {
    /// `<bare identifier> = <expression>`; the only assignment form in v1.
    Assign {
        /// Assigned identifier.
        name: String,
        /// Span of the identifier token (not the whole assignment).
        name_span: Span,
        /// Right-hand side expression.
        value: Expr,
    },
    /// A bare expression statement (typically a call).
    Expr(Expr),
}

/// A complete parsed statement: either an assignment or an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    /// Statement payload.
    pub kind: StatementKind,
    /// Span of the whole statement, from the first token through the last.
    pub span: Span,
}

impl Statement {
    /// Span of the whole statement, from the first token through the last.
    pub fn span(&self) -> Span {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_expression;

    #[test]
    fn span_accessor_covers_structural_nodes() {
        let expr = parse_expression("f(1, r=2)").unwrap();
        assert_eq!(expr.span(), Span::new(0, 9));
        let Expr::Call { callee, args, .. } = &expr else {
            panic!("expected call");
        };
        assert_eq!(callee.span(), Span::new(0, 1));
        assert_eq!(args[0].span, Span::new(2, 3));
        assert_eq!(args[1].span, Span::new(5, 8));
        assert_eq!(args[1].name.as_deref(), Some("r"));
    }

    #[test]
    fn spanless_variants_return_the_empty_span() {
        for expr in [
            Expr::Int(BigInt::from(1)),
            Expr::Float(1.0),
            Expr::Text("t".to_owned()),
            Expr::Bool(true),
            Expr::None,
            Expr::List(Vec::new()),
            Expr::Record(Vec::new()),
        ] {
            assert_eq!(expr.span(), Span::default());
        }
    }

    #[test]
    fn ast_types_serde_round_trip() {
        let expr = parse_expression(
            r#"f(1, [true, None], {键: 9.5e1}, 2 ** -1, a.b[0] == not false, 名="值")"#,
        )
        .unwrap();
        let json = serde_json::to_string(&expr).unwrap();
        let decoded: Expr = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, expr);

        let stmt = crate::parse_statement("结果 = 打印(阶乘(n=3))").unwrap();
        let json = serde_json::to_string(&stmt).unwrap();
        let decoded: Statement = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, stmt);
        assert_eq!(decoded.span(), stmt.span());

        let args = crate::parse_arguments("(r=3, precision=2)").unwrap();
        let json = serde_json::to_string(&args).unwrap();
        let decoded: Vec<Arg> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, args);
    }

    #[test]
    fn span_serde_round_trips_inside_ast() {
        let expr = parse_expression("a.b").unwrap();
        let json = serde_json::to_string(&expr).unwrap();
        assert!(json.contains("\"start\":0"));
        assert!(json.contains("\"end\":3"));
    }
}
