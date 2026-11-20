//! Recursive-descent parser for the Marqdo expression grammar v1.
//!
//! The parser is a straightforward implementation of `spec/expression.ebnf`:
//!
//! | level | operators | associativity |
//! |---|---|---|
//! | `or_expr` | `or` | left |
//! | `and_expr` | `and` | left |
//! | `compare_expr` | `== != < <= > >=` | non-associative |
//! | `add_expr` | `+ -` | left |
//! | `mul_expr` | `* / // %` | left |
//! | `unary_expr` | `not - +` | prefix, right |
//! | `power_expr` | `**` | right |
//! | `postfix_expr` | call, `.name`, `[expr]` | left |
//!
//! Nesting is bounded by [`MAX_NESTING_DEPTH`]; the guard is checked before
//! recursing into any sub-expression so adversarial inputs fail with a
//! structured [`ParseError`] instead of overflowing the stack.

use crate::ast::{
    Arg, AssignTarget, BinaryOp, Expr, Ident, RecordEntry, Statement, StatementKind, UnaryOp,
};
use crate::lexer::{self, Token, TokenKind};
use crate::ParseError;
use marqdo_span::Span;
use num_bigint::BigInt;

/// Maximum number of nested expression constructs accepted by the parser.
///
/// The top-level expression is depth 0; every nested grouping, list/record
/// element, call argument, index expression, prefix operator and power
/// exponent adds one. The 129th nested construct is rejected with an
/// `expr.parse` error.
pub const MAX_NESTING_DEPTH: usize = 128;

pub(crate) fn parse_expression(source: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser::new(source)?;
    if parser.at_eof() {
        return Err(parser.error_here("expected an expression, found end of input"));
    }
    let expr = parser.expression()?;
    parser.expect_end_of_expression()?;
    Ok(expr)
}

pub(crate) fn parse_statement(source: &str) -> Result<Statement, ParseError> {
    let mut parser = Parser::new(source)?;
    if parser.at_eof() {
        return Err(parser.error_here("expected a statement, found end of input"));
    }
    let start = parser.peek().span.start;
    let left = parser.expression()?;
    if matches!(parser.peek().kind, TokenKind::Assign) {
        return parser.assignment(start, left);
    }
    if !parser.at_eof() {
        return Err(parser.trailing_statement_error());
    }
    let end = parser.previous_end();
    Ok(Statement {
        kind: StatementKind::Expr(left),
        span: Span::new(start, end),
    })
}

pub(crate) fn parse_arguments(source: &str) -> Result<Vec<Arg>, ParseError> {
    let mut parser = Parser::new(source)?;
    if parser.at_eof() {
        return Err(
            parser.error_here("expected an argument list starting with `(`, found end of input")
        );
    }
    if !matches!(parser.peek().kind, TokenKind::LParen) {
        return Err(parser.error_here(format!(
            "expected an argument list starting with `(`, found {}",
            parser.describe_current()
        )));
    }
    let args = parser.call_arguments()?;
    if !parser.at_eof() {
        return Err(parser.error_here(format!(
            "unexpected {} after argument list",
            parser.describe_current()
        )));
    }
    Ok(args)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    depth: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self, ParseError> {
        Ok(Self {
            tokens: lexer::tokenize(source)?,
            index: 0,
            depth: 0,
        })
    }

    // ------------------------------------------------------------------
    // Token cursor helpers
    // ------------------------------------------------------------------

    fn peek(&self) -> &Token {
        // `tokens` always contains at least the EOF token, and `bump` never
        // advances past it, so this index is always in bounds.
        &self.tokens[self.index]
    }

    fn peek_n(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.index + offset)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn previous_end(&self) -> usize {
        if self.index == 0 {
            0
        } else {
            self.tokens[self.index - 1].span.end
        }
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.peek().span, message)
    }

    fn describe_current(&self) -> String {
        describe_token(&self.peek().kind)
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, ParseError> {
        if self.peek().kind == kind {
            Ok(self.bump())
        } else {
            Err(self.error_here(format!(
                "expected {what}, found {}",
                self.describe_current()
            )))
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<Ident, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                let token = self.bump();
                Ok(Ident {
                    name,
                    span: token.span,
                })
            }
            _ if is_keyword(&self.peek().kind) => Err(self.error_here(format!(
                "keywords are reserved and cannot be used as identifiers; found {}",
                self.describe_current()
            ))),
            _ => Err(self.error_here(format!(
                "expected {context}, found {}",
                self.describe_current()
            ))),
        }
    }

    // ------------------------------------------------------------------
    // Nesting guard
    // ------------------------------------------------------------------

    fn nested<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            self.depth -= 1;
            return Err(self.error_here(format!(
                "expression nesting exceeds the maximum depth of {MAX_NESTING_DEPTH}"
            )));
        }
        let result = parse(self);
        self.depth -= 1;
        result
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn assignment(&mut self, start: usize, left: Expr) -> Result<Statement, ParseError> {
        let target = self.assign_target(left)?;
        self.bump(); // `=`
        if self.at_eof() {
            return Err(self.error_here("expected an expression after `=`"));
        }
        let value = self.expression()?;
        if !self.at_eof() {
            return Err(if matches!(self.peek().kind, TokenKind::Assign) {
                self.error_here(
                    "chained assignment is not supported; assignment is only allowed at \
                     statement start",
                )
            } else {
                self.error_here(format!(
                    "unexpected {} after assignment value",
                    self.describe_current()
                ))
            });
        }
        let end = self.previous_end();
        Ok(Statement {
            kind: StatementKind::Assign { target, value },
            span: Span::new(start, end),
        })
    }

    /// Turn the already-parsed left expression into a writable target.
    ///
    /// v1 only writes local slots. Addressable `Memory` adds index targets;
    /// member assignment and every other shape stay hard parse errors so an
    /// invalid executable shape never silently becomes prose (ADR-0022).
    fn assign_target(&self, left: Expr) -> Result<AssignTarget, ParseError> {
        match left {
            Expr::Ident(Ident { name, span }) => Ok(AssignTarget::Local {
                name,
                name_span: span,
            }),
            Expr::Index { base, index, span } => Ok(AssignTarget::Index { base, index, span }),
            Expr::Member { span, .. } => Err(ParseError::new(
                span,
                "member assignment is not supported in v1; only a local name or a \
                 Memory index is writable",
            )),
            other => Err(ParseError::new(
                other.span(),
                format!(
                    "invalid assignment target: {} is not a writable place; assign a local \
                     name or a Memory index",
                    describe_expr(&other)
                ),
            )),
        }
    }

    fn trailing_statement_error(&self) -> ParseError {
        self.error_here(format!(
            "unexpected {} after expression",
            self.describe_current()
        ))
    }

    fn expect_end_of_expression(&self) -> Result<(), ParseError> {
        if self.at_eof() {
            Ok(())
        } else if matches!(self.peek().kind, TokenKind::Assign) {
            Err(self.error_here(
                "unexpected `=`; assignment is only supported as a statement, not inside an \
                 expression",
            ))
        } else {
            Err(self.error_here(format!(
                "unexpected {} after expression",
                self.describe_current()
            )))
        }
    }

    // ------------------------------------------------------------------
    // Expression levels, loosest to tightest
    // ------------------------------------------------------------------

    fn expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let mut left = self.parse_and()?;
        while matches!(self.peek().kind, TokenKind::Or) {
            self.bump();
            let right = self.parse_and()?;
            let end = self.previous_end();
            left = binary(BinaryOp::Or, left, right, Span::new(start, end));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let mut left = self.parse_compare()?;
        while matches!(self.peek().kind, TokenKind::And) {
            self.bump();
            let right = self.parse_compare()?;
            let end = self.previous_end();
            left = binary(BinaryOp::And, left, right, Span::new(start, end));
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let left = self.parse_add()?;
        let Some(op) = compare_op(&self.peek().kind) else {
            return Ok(left);
        };
        self.bump();
        let right = self.parse_add()?;
        let end = self.previous_end();
        let expr = binary(op, left, right, Span::new(start, end));
        if compare_op(&self.peek().kind).is_some() {
            return Err(self.error_here(format!(
                "comparison operators are non-associative; found {} after a comparison (join \
                 comparisons with `and`)",
                self.describe_current()
            )));
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let right = self.parse_mul()?;
            let end = self.previous_end();
            left = binary(op, left, right, Span::new(start, end));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::SlashSlash => BinaryOp::FloorDiv,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.bump();
            let right = self.parse_unary()?;
            let end = self.previous_end();
            left = binary(op, left, right, Span::new(start, end));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let op = match self.peek().kind {
            TokenKind::Not => Some(UnaryOp::Not),
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Plus => Some(UnaryOp::Pos),
            _ => None,
        };
        let Some(op) = op else {
            return self.parse_power();
        };
        let start = self.peek().span.start;
        self.bump();
        let operand = self.nested(|parser| parser.parse_unary())?;
        let end = self.previous_end();
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
            span: Span::new(start, end),
        })
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let base = self.parse_postfix()?;
        if matches!(self.peek().kind, TokenKind::StarStar) {
            self.bump();
            let exponent = self.nested(|parser| parser.parse_unary())?;
            let end = self.previous_end();
            return Ok(binary(BinaryOp::Pow, base, exponent, Span::new(start, end)));
        }
        Ok(base)
    }

    // ------------------------------------------------------------------
    // Postfix chain
    // ------------------------------------------------------------------

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span.start;
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.peek().kind, TokenKind::LParen) {
                let args = self.call_arguments()?;
                let end = self.previous_end();
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span: Span::new(start, end),
                };
            } else if matches!(self.peek().kind, TokenKind::Dot) {
                self.bump();
                let member = self.expect_identifier("a member name after `.`")?;
                let end = member.span.end;
                expr = Expr::Member {
                    base: Box::new(expr),
                    name: member.name,
                    span: Span::new(start, end),
                };
            } else if matches!(self.peek().kind, TokenKind::LBracket) {
                self.bump();
                let index = self.nested(|parser| parser.expression())?;
                let close = self.expect(TokenKind::RBracket, "`]`")?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                    span: Span::new(start, close.span.end),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    /// Parse `( [argument {"," argument} [","]] )`.
    ///
    /// Shared by call postfixes and by [`crate::parse_arguments`].
    fn call_arguments(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.expect(TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        if matches!(self.peek().kind, TokenKind::RParen) {
            self.bump();
            return Ok(args);
        }
        let mut seen_named = false;
        let mut names: Vec<String> = Vec::new();
        loop {
            let start = self.peek().span.start;
            let mut name = None;
            if let Some((candidate, name_span)) = self.peek_named_argument() {
                seen_named = true;
                if names.iter().any(|existing| existing == &candidate) {
                    return Err(ParseError::new(
                        name_span,
                        format!("duplicate named argument `{candidate}`"),
                    ));
                }
                self.bump(); // identifier
                self.bump(); // `=`
                names.push(candidate.clone());
                name = Some(candidate);
            } else if seen_named {
                return Err(
                    self.error_here("positional arguments must all precede named arguments")
                );
            }
            if matches!(self.peek().kind, TokenKind::Comma | TokenKind::RParen) {
                return Err(self.error_here(format!(
                    "expected an argument expression, found {}",
                    self.describe_current()
                )));
            }
            let value = self.nested(|parser| parser.expression())?;
            let end = self.previous_end();
            args.push(Arg {
                name,
                value,
                span: Span::new(start, end),
            });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    self.bump();
                    break;
                }
            } else if matches!(self.peek().kind, TokenKind::RParen) {
                self.bump();
                break;
            } else {
                return Err(self.error_here(format!(
                    "expected `,` or `)` in argument list, found {}",
                    self.describe_current()
                )));
            }
        }
        Ok(args)
    }

    fn peek_named_argument(&self) -> Option<(String, Span)> {
        let name = match &self.peek().kind {
            TokenKind::Ident(name) => name.clone(),
            _ => return None,
        };
        match self.peek_n(1).map(|token| &token.kind) {
            Some(TokenKind::Assign) => Some((name, self.peek().span)),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Primaries
    // ------------------------------------------------------------------

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Int(raw) => {
                self.bump();
                let value = BigInt::parse_bytes(raw.as_bytes(), 10)
                    .ok_or_else(|| ParseError::new(token.span, "invalid integer literal"))?;
                Ok(Expr::Int(value))
            }
            TokenKind::Float(raw) => {
                self.bump();
                self.float_literal(&raw, token.span)
            }
            TokenKind::Text(text) => {
                self.bump();
                Ok(Expr::Text(text))
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::Bool(false))
            }
            TokenKind::NoneKw => {
                self.bump();
                Ok(Expr::None)
            }
            TokenKind::Ident(name) => {
                self.bump();
                Ok(Expr::Ident(Ident {
                    name,
                    span: token.span,
                }))
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.nested(|parser| parser.expression())?;
                self.expect(TokenKind::RParen, "`)`")?;
                Ok(inner)
            }
            TokenKind::LBracket => self.list_literal(),
            TokenKind::LBrace => self.record_literal(),
            _ => Err(self.error_here(format!(
                "expected an expression, found {}",
                self.describe_current()
            ))),
        }
    }

    fn float_literal(&self, raw: &str, span: Span) -> Result<Expr, ParseError> {
        let value = raw
            .parse::<f64>()
            .map_err(|_| ParseError::new(span, format!("invalid float literal `{raw}`")))?;
        if !value.is_finite() {
            return Err(ParseError::new(
                span,
                format!(
                    "float literal `{raw}` is out of range for IEEE-754 binary64; infinities are \
                     only allowed as operation results"
                ),
            ));
        }
        Ok(Expr::Float(value))
    }

    fn list_literal(&mut self) -> Result<Expr, ParseError> {
        self.bump(); // `[`
        let mut items = Vec::new();
        if matches!(self.peek().kind, TokenKind::RBracket) {
            self.bump();
            return Ok(Expr::List(items));
        }
        loop {
            let item = self.nested(|parser| parser.expression())?;
            items.push(item);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RBracket) {
                    self.bump();
                    break;
                }
            } else if matches!(self.peek().kind, TokenKind::RBracket) {
                self.bump();
                break;
            } else {
                return Err(self.error_here(format!(
                    "expected `,` or `]` in list literal, found {}",
                    self.describe_current()
                )));
            }
        }
        Ok(Expr::List(items))
    }

    fn record_literal(&mut self) -> Result<Expr, ParseError> {
        self.bump(); // `{`
        let mut entries = Vec::new();
        if matches!(self.peek().kind, TokenKind::RBrace) {
            self.bump();
            return Ok(Expr::Record(entries));
        }
        loop {
            let (key, key_span) = self.record_key()?;
            self.expect(TokenKind::Colon, "`:` after a record key")?;
            if matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Comma) {
                return Err(self.error_here(format!(
                    "expected a record value expression, found {}",
                    self.describe_current()
                )));
            }
            let value = self.nested(|parser| parser.expression())?;
            entries.push(RecordEntry {
                key,
                key_span,
                value,
            });
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RBrace) {
                    self.bump();
                    break;
                }
            } else if matches!(self.peek().kind, TokenKind::RBrace) {
                self.bump();
                break;
            } else {
                return Err(self.error_here(format!(
                    "expected `,` or `}}` in record literal, found {}",
                    self.describe_current()
                )));
            }
        }
        Ok(Expr::Record(entries))
    }

    fn record_key(&mut self) -> Result<(String, Span), ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                let token = self.bump();
                Ok((name, token.span))
            }
            TokenKind::Text(text) => {
                let token = self.bump();
                Ok((text, token.span))
            }
            _ if is_keyword(&self.peek().kind) => Err(self.error_here(format!(
                "record keys must be identifiers or text literals; {} is a reserved keyword \
                 (quote it as a text literal)",
                self.describe_current()
            ))),
            _ => Err(self.error_here(format!(
                "expected a record key (identifier or text literal), found {}",
                self.describe_current()
            ))),
        }
    }
}

fn binary(op: BinaryOp, left: Expr, right: Expr, span: Span) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }
}

fn compare_op(kind: &TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::EqEq => Some(BinaryOp::Eq),
        TokenKind::Ne => Some(BinaryOp::Ne),
        TokenKind::Lt => Some(BinaryOp::Lt),
        TokenKind::Le => Some(BinaryOp::Le),
        TokenKind::Gt => Some(BinaryOp::Gt),
        TokenKind::Ge => Some(BinaryOp::Ge),
        _ => None,
    }
}

fn is_keyword(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::True
            | TokenKind::False
            | TokenKind::NoneKw
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Not
    )
}

fn describe_expr(expr: &Expr) -> &'static str {
    match expr {
        Expr::Int(_) => "an integer literal",
        Expr::Float(_) => "a float literal",
        Expr::Text(_) => "a text literal",
        Expr::Bool(_) => "a boolean literal",
        Expr::None => "`None`",
        Expr::Ident(_) => "an identifier",
        Expr::List(_) => "a list literal",
        Expr::Record(_) => "a record literal",
        Expr::Unary { .. } => "a unary expression",
        Expr::Binary { .. } => "a binary expression",
        Expr::Call { .. } => "a call expression",
        Expr::Member { .. } => "a member access",
        Expr::Index { .. } => "an index expression",
    }
}

fn describe_token(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Int(raw) => format!("integer literal `{raw}`"),
        TokenKind::Float(raw) => format!("float literal `{raw}`"),
        TokenKind::Text(_) => "text literal".to_owned(),
        TokenKind::Ident(name) => format!("identifier `{name}`"),
        TokenKind::True => "`true`".to_owned(),
        TokenKind::False => "`false`".to_owned(),
        TokenKind::NoneKw => "`None`".to_owned(),
        TokenKind::And => "`and`".to_owned(),
        TokenKind::Or => "`or`".to_owned(),
        TokenKind::Not => "`not`".to_owned(),
        TokenKind::LParen => "`(`".to_owned(),
        TokenKind::RParen => "`)`".to_owned(),
        TokenKind::LBracket => "`[`".to_owned(),
        TokenKind::RBracket => "`]`".to_owned(),
        TokenKind::LBrace => "`{`".to_owned(),
        TokenKind::RBrace => "`}`".to_owned(),
        TokenKind::Comma => "`,`".to_owned(),
        TokenKind::Colon => "`:`".to_owned(),
        TokenKind::Dot => "`.`".to_owned(),
        TokenKind::Assign => "`=`".to_owned(),
        TokenKind::EqEq => "`==`".to_owned(),
        TokenKind::Ne => "`!=`".to_owned(),
        TokenKind::Lt => "`<`".to_owned(),
        TokenKind::Le => "`<=`".to_owned(),
        TokenKind::Gt => "`>`".to_owned(),
        TokenKind::Ge => "`>=`".to_owned(),
        TokenKind::Plus => "`+`".to_owned(),
        TokenKind::Minus => "`-`".to_owned(),
        TokenKind::Star => "`*`".to_owned(),
        TokenKind::StarStar => "`**`".to_owned(),
        TokenKind::Slash => "`/`".to_owned(),
        TokenKind::SlashSlash => "`//`".to_owned(),
        TokenKind::Percent => "`%`".to_owned(),
        TokenKind::Eof => "end of input".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_arguments, parse_expression, parse_statement, ParseError};
    use std::str::FromStr;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn ex(source: &str) -> Expr {
        parse_expression(source)
            .unwrap_or_else(|err| panic!("{source:?} should parse but failed: {err}"))
    }

    fn ex_err(source: &str) -> ParseError {
        match parse_expression(source) {
            Ok(expr) => panic!("{source:?} should fail but parsed as {expr:?}"),
            Err(err) => err,
        }
    }

    fn st(source: &str) -> Statement {
        parse_statement(source)
            .unwrap_or_else(|err| panic!("{source:?} should parse as a statement: {err}"))
    }

    fn st_err(source: &str) -> ParseError {
        match parse_statement(source) {
            Ok(stmt) => panic!("{source:?} should fail but parsed as {stmt:?}"),
            Err(err) => err,
        }
    }

    /// Compact span-insensitive rendering, used to assert tree shape.
    fn sx(source: &str) -> String {
        sexpr(&ex(source))
    }

    fn sexpr(expr: &Expr) -> String {
        match expr {
            Expr::Int(value) => value.to_string(),
            Expr::Float(value) => format!("{value:?}"),
            Expr::Text(value) => format!("{value:?}"),
            Expr::Bool(value) => value.to_string(),
            Expr::None => "None".to_owned(),
            Expr::Ident(ident) => ident.name.clone(),
            Expr::List(items) => {
                let mut out = String::from("(list");
                for item in items {
                    out.push(' ');
                    out.push_str(&sexpr(item));
                }
                out.push(')');
                out
            }
            Expr::Record(entries) => {
                let mut out = String::from("(record");
                for entry in entries {
                    out.push_str(&format!(" {}: {}", entry.key, sexpr(&entry.value)));
                }
                out.push(')');
                out
            }
            Expr::Unary { op, operand, .. } => {
                format!("({} {})", unary_symbol(*op), sexpr(operand))
            }
            Expr::Binary {
                op, left, right, ..
            } => format!("({} {} {})", binary_symbol(*op), sexpr(left), sexpr(right)),
            Expr::Call { callee, args, .. } => {
                let mut out = format!("(call {}", sexpr(callee));
                for arg in args {
                    match &arg.name {
                        Some(name) => out.push_str(&format!(" {name}={}", sexpr(&arg.value))),
                        None => {
                            out.push(' ');
                            out.push_str(&sexpr(&arg.value));
                        }
                    }
                }
                out.push(')');
                out
            }
            Expr::Member { base, name, .. } => format!("(. {} {name})", sexpr(base)),
            Expr::Index { base, index, .. } => {
                format!("(index {} {})", sexpr(base), sexpr(index))
            }
        }
    }

    fn binary_symbol(op: BinaryOp) -> &'static str {
        match op {
            BinaryOp::Or => "or",
            BinaryOp::And => "and",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::FloorDiv => "//",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "**",
        }
    }

    fn unary_symbol(op: UnaryOp) -> &'static str {
        match op {
            UnaryOp::Not => "not",
            UnaryOp::Neg => "neg",
            UnaryOp::Pos => "pos",
        }
    }

    // ------------------------------------------------------------------
    // Literals
    // ------------------------------------------------------------------

    #[test]
    fn integer_literals_are_arbitrary_precision() {
        assert_eq!(ex("0"), Expr::Int(BigInt::from(0)));
        assert_eq!(ex("007"), Expr::Int(BigInt::from(7)));
        assert_eq!(
            ex("9223372036854775808"),
            Expr::Int(BigInt::from(9_223_372_036_854_775_808u64))
        );
        let huge = "123456789012345678901234567890123456789012345678901234567890";
        assert_eq!(ex(huge), Expr::Int(BigInt::from_str(huge).unwrap()));
        assert_eq!(sx("-9223372036854775809"), "(neg 9223372036854775809)");
    }

    #[test]
    fn float_literals_cover_the_grammar() {
        assert_eq!(ex("1.5"), Expr::Float(1.5));
        assert_eq!(ex("0.0"), Expr::Float(0.0));
        assert_eq!(ex("1.5e3"), Expr::Float(1500.0));
        assert_eq!(ex("1.25E-2"), Expr::Float(0.0125));
        assert_eq!(ex("0.5e+2"), Expr::Float(50.0));
        assert_eq!(
            ex("-2.5"),
            Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(Expr::Float(2.5)),
                span: Span::new(0, 4),
            }
        );
    }

    #[test]
    fn invalid_float_literals_are_rejected() {
        for bad in [
            ".5", "1.", "1e5", "1.2.3", "1.5e", "1.5e+", "1_000", "1.0e999",
        ] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
            assert!(!err.message.is_empty(), "input {bad:?}");
        }
        // `.5` gets the specific "must be preceded" diagnostic from the lexer.
        assert!(ex_err(".5").message.contains("preceded"));
        assert!(ex_err("1.0e999").message.contains("out of range"));
    }

    #[test]
    fn text_literals_decode_escapes() {
        assert_eq!(ex(r#""hello""#), Expr::Text("hello".to_owned()));
        assert_eq!(ex(r#""""#), Expr::Text(String::new()));
        assert_eq!(
            ex(r#""a\\b\"c\n\t\r\u{4e2d}""#),
            Expr::Text("a\\b\"c\n\t\r中".to_owned())
        );
        assert_eq!(ex(r#""\u{4E2D}\u{1F600}""#), Expr::Text("中😀".to_owned()));
        // A raw newline is a valid text character inside a literal.
        assert_eq!(ex("\"a\nb\""), Expr::Text("a\nb".to_owned()));
        assert_eq!(ex(r#""{}()[],:.""#), Expr::Text("{}()[],:.".to_owned()));
    }

    #[test]
    fn invalid_text_literals_are_rejected() {
        for bad in [
            r#""unterminated"#,
            r#""bad \q escape""#,
            r#""bad \u{} escape""#,
            r#""bad \u{D800} escape""#,
            r#""bad \u{110000} escape""#,
            r#""bad \u12 escape""#,
        ] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        // `\\` at the end of the input is an unterminated literal.
        let err = parse_expression("\"abc\\").unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn boolean_and_none_literals_parse() {
        assert_eq!(ex("true"), Expr::Bool(true));
        assert_eq!(ex("false"), Expr::Bool(false));
        assert_eq!(ex("None"), Expr::None);
        // `truex` is an identifier, not the keyword `true` plus `x`.
        assert_eq!(sx("truex"), "truex");
        assert_eq!(sx("None_"), "None_");
    }

    #[test]
    fn unicode_identifiers_are_accepted() {
        assert_eq!(sx("阶乘"), "阶乘");
        assert_eq!(sx("计算_面积2"), "计算_面积2");
        assert_eq!(sx("café"), "café");
        assert!(matches!(ex("π"), Expr::Ident(_)));
        // U+005F `_` is not XID_Start, so it cannot start an identifier.
        let _ = st("变量 = 1").kind;
        for bad in ["_x", "__", "1abc"] {
            assert_eq!(ex_err(bad).code, "expr.parse", "input {bad:?}");
        }
    }

    #[test]
    fn reserved_keywords_cannot_be_identifiers() {
        for bad in ["and", "or", "not", "true = 1"] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        assert!(ex_err("a.and").message.contains("reserved"));
        assert!(ex_err("a.true").message.contains("reserved"));
    }

    // ------------------------------------------------------------------
    // Operator precedence and associativity
    // ------------------------------------------------------------------

    #[test]
    fn additive_and_multiplicative_precedence() {
        assert_eq!(sx("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(sx("(1 + 2) * 3"), "(* (+ 1 2) 3)");
        assert_eq!(sx("1 * 2 + 3"), "(+ (* 1 2) 3)");
        assert_eq!(sx("1 - 2 - 3"), "(- (- 1 2) 3)");
        assert_eq!(sx("8 / 4 // 2 % 3"), "(% (// (/ 8 4) 2) 3)");
        assert_eq!(sx("1 + 2 + 3"), "(+ (+ 1 2) 3)");
    }

    #[test]
    fn power_is_right_associative_and_binds_tighter_than_unary_minus() {
        assert_eq!(sx("2 ** 3 ** 2"), "(** 2 (** 3 2))");
        assert_eq!(sx("-x ** 2"), "(neg (** x 2))");
        assert_eq!(sx("(-x) ** 2"), "(** (neg x) 2)");
        assert_eq!(sx("2 ** -1"), "(** 2 (neg 1))");
        assert_eq!(sx("2 ** -1 ** 2"), "(** 2 (neg (** 1 2)))");
        assert_eq!(sx("+x ** 2"), "(pos (** x 2))");
        assert_eq!(sx("a * b ** c * d"), "(* (* a (** b c)) d)");
    }

    #[test]
    fn unary_operators_stack() {
        assert_eq!(sx("--1"), "(neg (neg 1))");
        assert_eq!(sx("not not true"), "(not (not true))");
        assert_eq!(sx("+1"), "(pos 1)");
        assert_eq!(sx("-+1"), "(neg (pos 1))");
        assert_eq!(sx("not a"), "(not a)");
        assert!(ex_err("not").message.contains("expected an expression"));
        assert!(ex_err("!a").message.contains("`not`"));
    }

    #[test]
    fn comparisons_bind_looser_than_arithmetic() {
        assert_eq!(sx("a < b + c"), "(< a (+ b c))");
        assert_eq!(sx("a + b < c * d"), "(< (+ a b) (* c d))");
        assert_eq!(sx("a == b"), "(== a b)");
        assert_eq!(sx("a != b"), "(!= a b)");
        assert_eq!(sx("a <= b"), "(<= a b)");
        assert_eq!(sx("a > b"), "(> a b)");
        assert_eq!(sx("a >= b"), "(>= a b)");
        // `not` binds tighter than comparison.
        assert_eq!(sx("not a == b"), "(== (not a) b)");
        assert_eq!(sx("-a < b"), "(< (neg a) b)");
    }

    #[test]
    fn comparisons_are_non_associative() {
        let err = ex_err("a < b < c");
        assert!(err.message.contains("non-associative"));
        assert_eq!(err.span, Span::new(6, 7));
        assert!(ex_err("a == b == c").message.contains("non-associative"));
        assert!(ex_err("a <= b > c").message.contains("non-associative"));
        // The documented workaround parses.
        assert_eq!(sx("a < b and b < c"), "(and (< a b) (< b c))");
    }

    #[test]
    fn logical_operators_chain_left_associatively() {
        assert_eq!(sx("a and b or c"), "(or (and a b) c)");
        assert_eq!(sx("a or b and c"), "(or a (and b c))");
        assert_eq!(sx("a or b or c"), "(or (or a b) c)");
        assert_eq!(sx("a and b and c"), "(and (and a b) c)");
        assert_eq!(sx("not a and not b"), "(and (not a) (not b))");
        assert_eq!(sx("a and b == c"), "(and a (== b c))");
        assert_eq!(sx("a == b and c != d"), "(and (== a b) (!= c d))");
    }

    // ------------------------------------------------------------------
    // Calls, members, indexes
    // ------------------------------------------------------------------

    #[test]
    fn calls_parse_positional_and_named_arguments() {
        assert_eq!(sx("f()"), "(call f)");
        assert_eq!(sx("f(1, 2)"), "(call f 1 2)");
        assert_eq!(sx("f(1,)"), "(call f 1)");
        assert_eq!(sx("f(x=1)"), "(call f x=1)");
        assert_eq!(sx("f(1, y=2,)"), "(call f 1 y=2)");
        assert_eq!(sx("f(g(1), h(x=2))"), "(call f (call g 1) (call h x=2))");
        assert_eq!(sx(r#"打印("你好")"#), r#"(call 打印 "你好")"#);
    }

    #[test]
    fn call_argument_errors_are_rejected() {
        let err = ex_err("f(a=1, 2)");
        assert!(err.message.contains("positional arguments"));
        let err = ex_err("f(a=1, a=2)");
        assert!(err.message.contains("duplicate named argument"));
        for bad in ["f(,)", "f(1,,2)", "f(1 2)", "f(", "f(x=)", "f(a=1, b)"] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
    }

    #[test]
    fn member_and_index_access_chain() {
        assert_eq!(sx("a.b"), "(. a b)");
        assert_eq!(sx("a.b.c"), "(. (. a b) c)");
        assert_eq!(sx("a.b[0].c(d)"), "(call (. (index (. a b) 0) c) d)");
        assert_eq!(sx("a[0][1]"), "(index (index a 0) 1)");
        assert_eq!(sx("a[1 + 2]"), "(index a (+ 1 2))");
        assert_eq!(sx("f(1).name"), "(. (call f 1) name)");
    }

    #[test]
    fn member_and_index_errors_are_rejected() {
        for bad in ["a.", "a[]", "a[1", "a[1,2]", "a.1", "a[1]]"] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        assert!(ex_err("a.true").message.contains("reserved"));
    }

    // ------------------------------------------------------------------
    // Collection literals
    // ------------------------------------------------------------------

    #[test]
    fn list_literals_parse() {
        assert_eq!(sx("[]"), "(list)");
        assert_eq!(sx("[1]"), "(list 1)");
        assert_eq!(sx("[1, 2, 3]"), "(list 1 2 3)");
        assert_eq!(sx("[1,]"), "(list 1)");
        assert_eq!(sx("[a, [b, c]]"), "(list a (list b c))");
        assert_eq!(sx(r#"["文本", true, None]"#), r#"(list "文本" true None)"#);
    }

    #[test]
    fn list_literal_errors_are_rejected() {
        for bad in ["[1 2]", "[,]", "[1,,2]", "[1", "[1, ", "[1; 2]"] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
    }

    #[test]
    fn record_literals_parse() {
        assert_eq!(sx("{}"), "(record)");
        assert_eq!(sx("{a: 1}"), "(record a: 1)");
        assert_eq!(sx("{a: 1, b: 2,}"), "(record a: 1 b: 2)");
        assert_eq!(sx(r#"{"键": "值"}"#), r#"(record 键: "值")"#);
        assert_eq!(sx("{a: {b: [1]}}"), "(record a: (record b: (list 1)))");
        let expr = ex("{答案: 42}");
        match expr {
            Expr::Record(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].key, "答案");
                assert_eq!(entries[0].key_span, Span::new(1, 7));
            }
            other => panic!("expected a record, got {other:?}"),
        }
    }

    #[test]
    fn record_literal_errors_are_rejected() {
        for bad in ["{a 1}", "{true: 1}", "{a: }", "{1: 2}", "{a: 1,,}", "{"] {
            let err = ex_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        let err = ex_err("{true: 1}");
        assert!(err.message.contains("reserved keyword"));
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    #[test]
    fn assignment_statements_parse() {
        let stmt = st("x = 1");
        assert_eq!(
            stmt.kind,
            StatementKind::Assign {
                target: AssignTarget::Local {
                    name: "x".to_owned(),
                    name_span: Span::new(0, 1),
                },
                value: Expr::Int(BigInt::from(1)),
            }
        );
        let stmt = st("结果 = 阶乘(n=3)");
        match &stmt.kind {
            StatementKind::Assign {
                target: AssignTarget::Local { name, .. },
                value,
            } => {
                assert_eq!(name, "结果");
                assert_eq!(sexpr(value), "(call 阶乘 n=3)");
            }
            other => panic!("expected assignment, got {other:?}"),
        }
        assert_eq!(st("x = y + 1").span(), Span::new(0, 9));
    }

    #[test]
    fn memory_index_assignment_parses_as_a_place() {
        let stmt = st("m[8] = 42");
        let StatementKind::Assign { target, value } = &stmt.kind else {
            panic!("expected assignment, got {:?}", stmt.kind);
        };
        let AssignTarget::Index { base, index, span } = target else {
            panic!("expected index target, got {target:?}");
        };
        assert_eq!(
            **base,
            Expr::Ident(Ident {
                name: "m".to_owned(),
                span: Span::new(0, 1),
            })
        );
        assert_eq!(**index, Expr::Int(BigInt::from(8)));
        assert_eq!(*span, Span::new(0, 4));
        assert_eq!(*value, Expr::Int(BigInt::from(42)));
        assert_eq!(stmt.span, Span::new(0, 9));

        // Nested targets are syntactically places; the runtime decides whether
        // the base value is a writable Memory region.
        let stmt = st("outer[0][1] = 2");
        let StatementKind::Assign {
            target: AssignTarget::Index { base, .. },
            ..
        } = &stmt.kind
        else {
            panic!("expected nested index target, got {:?}", stmt.kind);
        };
        assert!(matches!(base.as_ref(), Expr::Index { .. }));
    }

    #[test]
    fn expression_statements_parse() {
        assert_eq!(st("打印(1)").kind, StatementKind::Expr(ex("打印(1)")));
        assert_eq!(st("x").kind, StatementKind::Expr(ex("x")));
        assert_eq!(st("x == 1").kind, StatementKind::Expr(ex("x == 1")));
    }

    #[test]
    fn invalid_statements_are_rejected() {
        for bad in [
            "",
            "   ",
            "x =",
            "x = 1 = 2",
            "a.b = 1",
            "1 = 2",
            "f() = 1",
            "= 1",
            "x = 1 2",
        ] {
            let err = st_err(bad);
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        assert!(st_err("x = 1 = 2").message.contains("chained assignment"));
        assert!(st_err("a.b = 1").message.contains("member assignment"));
        assert!(st_err("1 = 2").message.contains("writable place"));
        assert!(st_err("x =").message.contains("after `=`"));
    }

    #[test]
    fn expressions_reject_assignment_syntax() {
        assert!(ex_err("x = 1")
            .message
            .contains("assignment is only supported as a statement"));
    }

    #[test]
    fn comments_are_not_supported() {
        for bad in ["# comment", "1 # 2", "f() # tail"] {
            let err = ex_err(bad);
            assert!(err.message.contains("comments"), "input {bad:?}");
        }
    }

    // ------------------------------------------------------------------
    // Spans
    // ------------------------------------------------------------------

    #[test]
    fn spans_are_byte_offsets_into_the_payload() {
        let expr = ex("1 + 2");
        let Expr::Binary { span, .. } = expr else {
            panic!("expected binary");
        };
        assert_eq!(span, Span::new(0, 5));

        let stmt = st("  x = 1  ");
        assert_eq!(stmt.span(), Span::new(2, 7));
        match stmt.kind {
            StatementKind::Assign {
                target: AssignTarget::Local { name_span, .. },
                ..
            } => assert_eq!(name_span, Span::new(2, 3)),
            other => panic!("expected assignment, got {other:?}"),
        }

        let Expr::Call {
            callee, span, args, ..
        } = ex("f(1)")
        else {
            panic!("expected call");
        };
        assert_eq!(span, Span::new(0, 4));
        assert_eq!(callee.span(), Span::new(0, 1));
        assert_eq!(args[0].span, Span::new(2, 3));

        let Expr::Member { span, .. } = ex("a.b") else {
            panic!("expected member");
        };
        assert_eq!(span, Span::new(0, 3));

        let Expr::Index { span, .. } = ex("a[0]") else {
            panic!("expected index");
        };
        assert_eq!(span, Span::new(0, 4));

        let Expr::Unary { span, .. } = ex("-x") else {
            panic!("expected unary");
        };
        assert_eq!(span, Span::new(0, 2));

        let Expr::Binary { span, .. } = ex("2 ** 3") else {
            panic!("expected power");
        };
        assert_eq!(span, Span::new(0, 6));

        let Expr::Binary { span, .. } = ex("(1) + 2") else {
            panic!("expected binary");
        };
        assert_eq!(span, Span::new(0, 7));

        // Multibyte payloads use byte offsets (张三 is three bytes per scalar).
        let Expr::Binary { span, .. } = ex("张三 + 1") else {
            panic!("expected binary");
        };
        assert_eq!(span, Span::new(0, 10));

        // Literal variants have no stored span in the frozen AST shape.
        assert_eq!(ex("42").span(), Span::default());
        assert_eq!(ex("[1]").span(), Span::default());
        assert_eq!(ex("{a: 1}").span(), Span::default());
        assert_eq!(ex(r#""s""#).span(), Span::default());
    }

    #[test]
    fn error_spans_point_at_the_offending_token() {
        assert_eq!(ex_err("1 2").span, Span::new(2, 3));
        assert_eq!(ex_err("f(1 2)").span, Span::new(4, 5));
        assert_eq!(ex_err("1 +").span, Span::empty(3));
        assert_eq!(ex_err("f(").span, Span::empty(2));
        assert_eq!(ex_err("a < b < c").span, Span::new(6, 7));
        let err = ex_err("f(a=1, a=2)");
        assert_eq!(err.span, Span::new(7, 8)); // span of the duplicate name token
    }

    // ------------------------------------------------------------------
    // Nesting limit
    // ------------------------------------------------------------------

    fn nested_parens(depth: usize) -> String {
        format!("{}1{}", "(".repeat(depth), ")".repeat(depth))
    }

    #[test]
    fn nesting_depth_limit_is_enforced() {
        assert!(parse_expression(&nested_parens(MAX_NESTING_DEPTH)).is_ok());
        let err = parse_expression(&nested_parens(MAX_NESTING_DEPTH + 1)).unwrap_err();
        assert_eq!(err.code, "expr.parse");
        assert!(err.message.contains("nesting"));
        assert_eq!(
            err.span,
            Span::new(MAX_NESTING_DEPTH + 1, MAX_NESTING_DEPTH + 2)
        );

        // Each prefix operator nests its operand, so exactly
        // MAX_NESTING_DEPTH prefix operators are accepted.
        let ok_unary = format!("{}1", "-".repeat(MAX_NESTING_DEPTH));
        assert!(parse_expression(&ok_unary).is_ok());
        let bad_unary = format!("{}1", "-".repeat(MAX_NESTING_DEPTH + 1));
        assert!(parse_expression(&bad_unary)
            .unwrap_err()
            .message
            .contains("nesting"));

        let deep_list = format!("{}1{}", "[".repeat(1_000), "]".repeat(1_000));
        let err = parse_expression(&deep_list).unwrap_err();
        assert!(err.message.contains("nesting"));

        let deep_record = format!("{}1{}", "{a: ".repeat(1_000), "}".repeat(1_000));
        assert!(parse_expression(&deep_record)
            .unwrap_err()
            .message
            .contains("nesting"));
    }

    // ------------------------------------------------------------------
    // parse_arguments()
    // ------------------------------------------------------------------

    #[test]
    fn parse_arguments_parses_a_leading_argument_list() {
        assert!(parse_arguments("()").unwrap().is_empty());

        let args = parse_arguments("(r=3, precision=2)").unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name.as_deref(), Some("r"));
        assert_eq!(sexpr(&args[0].value), "3");
        assert_eq!(args[0].span, Span::new(1, 4));
        assert_eq!(args[1].name.as_deref(), Some("precision"));
        assert_eq!(args[1].span, Span::new(6, 17));

        let mixed = parse_arguments("(1, r=2,)").unwrap();
        assert_eq!(mixed.len(), 2);
        assert_eq!(mixed[0].name, None);
        assert_eq!(sexpr(&mixed[0].value), "1");
        assert_eq!(mixed[1].name.as_deref(), Some("r"));
        assert_eq!(mixed[1].value, Expr::Int(BigInt::from(2)));
    }

    #[test]
    fn parse_arguments_matches_call_argument_semantics() {
        let standalone = parse_arguments("(1, x=2)").unwrap();
        let via_call = match ex("f(1, x=2)") {
            Expr::Call { args, .. } => args,
            other => panic!("expected call, got {other:?}"),
        };
        assert_eq!(standalone.len(), via_call.len());
        assert_eq!(standalone[0].name, via_call[0].name);
        assert_eq!(sexpr(&standalone[0].value), sexpr(&via_call[0].value));
        assert_eq!(standalone[1].name, via_call[1].name);
        assert_eq!(sexpr(&standalone[1].value), sexpr(&via_call[1].value));
    }

    #[test]
    fn parse_arguments_rejects_invalid_lists() {
        assert!(parse_arguments("").is_err());
        assert!(parse_arguments("r=3").is_err());
        assert!(parse_arguments("(r=3) junk").is_err());
        assert!(parse_arguments("(1, r=2, 3)").is_err());
        assert!(parse_arguments("(").is_err());
        assert!(parse_arguments("()extra").is_err());
        assert!(parse_arguments("(1 2)").is_err());
        let err = parse_arguments("(a=1, a=2)").unwrap_err();
        assert!(err.message.contains("duplicate named argument"));
        let err = parse_arguments("(1, r=2, 3)").unwrap_err();
        assert!(err.message.contains("positional arguments"));
    }

    // ------------------------------------------------------------------
    // Adversarial inputs must never panic
    // ------------------------------------------------------------------

    #[test]
    fn adversarial_inputs_never_panic() {
        let inputs = [
            "",
            " ",
            "\t\n",
            "\\",
            "\"",
            "\u{0}",
            "\u{10FFFF}",
            "\u{feff}1",
            "1.2.3.4",
            "((((((((((",
            "))))))",
            "**",
            "//",
            "==",
            "!",
            "not not not",
            "{}[]()",
            "a..b",
            "a[[[",
            "[\\u{",
            "😀",
            "1e",
            "0x10",
            "...",
            "a = = b",
            "f(((((((((",
            "🎉 + 1",
            "{\"a\": }",
            "1234567890123456789012345678901234567890.0e-99999",
        ];
        for input in inputs {
            let _ = parse_expression(input);
            let _ = parse_statement(input);
            let _ = parse_arguments(input);
        }

        // A long flat expression parses without exhausting the stack.
        let long = format!("1{}", " + 1".repeat(2_000));
        assert!(parse_expression(&long).is_ok());
    }
}
