//! Lexer and recursive-descent parser for the documented math subset.
//!
//! The parser is intentionally strict: anything outside the subset becomes a
//! structured [`MathError`] instead of being skipped, guessed or silently
//! simplified. It never panics on untrusted input: recursion depth, source size
//! and numeric literal size are all bounded before the corresponding work
//! happens.

use std::iter::Peekable;
use std::str::CharIndices;

use num_bigint::BigInt;
use num_rational::Ratio;

use crate::ast::{Constant, Expr, Formula, FormulaKind, Rational};
use crate::error::MathError;

/// Maximum accepted source length in bytes. Longer inputs are rejected before
/// tokenization so that a hostile formula cannot consume unbounded memory.
pub const MAX_SOURCE_BYTES: usize = 1_000_000;

/// Maximum parser recursion depth (parentheses, groups, unary chains, fractions
/// and roots all consume one level). Deeper input is a parse error instead of a
/// stack overflow.
pub const MAX_PARSE_DEPTH: usize = 256;

/// Maximum number of AST nodes in one formula. This bounds evaluation,
/// substitution and rendering recursion depth: a long left-associative chain
/// such as `1 + 1 + ...` would otherwise build a very deep tree even though the
/// parser loop itself is iterative. Deeper formulas are rejected with a parse
/// error instead of risking a stack overflow later.
pub const MAX_AST_NODES: usize = 512;

/// Maximum number of digits in one numeric literal.
const MAX_LITERAL_DIGITS: usize = 100_000;

/// Plain identifiers that name elementary functions in ordinary LaTeX. They are
/// reported as unsupported instead of being silently reinterpreted as variables
/// or implicit multiplication.
const FUNCTION_NAMES: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "log", "ln", "lg", "exp", "abs", "min", "max", "floor", "ceil", "round", "det", "dim", "gcd",
    "lcm", "lim", "sup", "inf", "arg", "Re", "Im",
];

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number(Rational),
    Ident(String),
    Constant(Constant),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Equals,
    Frac,
    Sqrt,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    /// Byte offset of the first byte of the token.
    start: usize,
}

impl Token {
    fn describe(&self) -> String {
        match &self.kind {
            TokenKind::Number(_) => "a number".to_string(),
            TokenKind::Ident(name) => format!("identifier `{name}`"),
            TokenKind::Constant(constant) => match constant {
                Constant::Pi => "`\\pi`".to_string(),
                Constant::E => "`\\e`".to_string(),
            },
            TokenKind::Plus => "`+`".to_string(),
            TokenKind::Minus => "`-`".to_string(),
            TokenKind::Star => "`*`".to_string(),
            TokenKind::Slash => "`/`".to_string(),
            TokenKind::Caret => "`^`".to_string(),
            TokenKind::LParen => "`(`".to_string(),
            TokenKind::RParen => "`)`".to_string(),
            TokenKind::LBrace => "`{`".to_string(),
            TokenKind::RBrace => "`}`".to_string(),
            TokenKind::Equals => "`=`".to_string(),
            TokenKind::Frac => "`\\frac`".to_string(),
            TokenKind::Sqrt => "`\\sqrt`".to_string(),
            TokenKind::Eof => "the end of the formula".to_string(),
        }
    }
}

/// Parse `latex` into a [`Formula`].
///
/// The supported grammar is documented on the crate root. The returned formula
/// keeps the original source in [`Formula::source`] and exposes the parsed
/// [`FormulaKind`] for inspection.
pub fn parse(latex: &str) -> Result<Formula, MathError> {
    if latex.len() > MAX_SOURCE_BYTES {
        return Err(MathError::parse(
            format!(
                "formula source is {} bytes; the supported limit is {MAX_SOURCE_BYTES} bytes",
                latex.len()
            ),
            Some(0),
        ));
    }
    let tokens = lex(latex)?;
    Parser {
        tokens,
        position: 0,
        depth: 0,
        nodes: 0,
    }
    .parse_formula(latex)
}

fn lex(source: &str) -> Result<Vec<Token>, MathError> {
    Lexer {
        source,
        chars: source.char_indices().peekable(),
        tokens: Vec::new(),
    }
    .run()
}

struct Lexer<'a> {
    source: &'a str,
    chars: Peekable<CharIndices<'a>>,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(mut self) -> Result<Vec<Token>, MathError> {
        while let Some(&(start, ch)) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
                continue;
            }
            let kind = match ch {
                '+' => {
                    self.chars.next();
                    TokenKind::Plus
                }
                '-' => {
                    self.chars.next();
                    TokenKind::Minus
                }
                '*' => {
                    self.chars.next();
                    TokenKind::Star
                }
                '/' => {
                    self.chars.next();
                    TokenKind::Slash
                }
                '^' => {
                    self.chars.next();
                    TokenKind::Caret
                }
                '(' => {
                    self.chars.next();
                    TokenKind::LParen
                }
                ')' => {
                    self.chars.next();
                    TokenKind::RParen
                }
                '{' => {
                    self.chars.next();
                    TokenKind::LBrace
                }
                '}' => {
                    self.chars.next();
                    TokenKind::RBrace
                }
                '=' => {
                    self.chars.next();
                    TokenKind::Equals
                }
                '\\' => self.lex_command()?,
                ch if ch.is_ascii_digit() || ch == '.' => self.lex_number()?,
                ch if is_ident_start(ch) => self.lex_identifier()?,
                other => {
                    return Err(MathError::unsupported(
                        format!("unsupported character `{other}` in a formula"),
                        Some(start),
                    ));
                }
            };
            self.tokens.push(Token { kind, start });
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            start: self.source.len(),
        });
        Ok(self.tokens)
    }

    fn lex_identifier(&mut self) -> Result<TokenKind, MathError> {
        let start = self.current_offset();
        let mut text = String::new();
        while let Some(&(_, ch)) = self.chars.peek() {
            if is_ident_continue(ch) {
                text.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        match text.as_str() {
            "pi" => Ok(TokenKind::Constant(Constant::Pi)),
            "e" => Ok(TokenKind::Constant(Constant::E)),
            _ if FUNCTION_NAMES.contains(&text.as_str()) => Err(MathError::unsupported(
                format!(
                    "function `{text}` is not supported; the native math subset has no function calls"
                ),
                Some(start),
            )),
            _ => Ok(TokenKind::Ident(text)),
        }
    }

    /// Byte offset of the next unconsumed character (or the source end).
    fn current_offset(&mut self) -> usize {
        match self.chars.peek() {
            Some(&(offset, _)) => offset,
            None => self.source.len(),
        }
    }

    fn lex_number(&mut self) -> Result<TokenKind, MathError> {
        let start = self.current_offset();
        let mut integer_digits = String::new();
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_ascii_digit() {
                integer_digits.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }

        let mut fraction_digits = String::new();
        if matches!(self.chars.peek(), Some(&(_, '.'))) {
            self.chars.next();
            while let Some(&(_, ch)) = self.chars.peek() {
                if ch.is_ascii_digit() {
                    fraction_digits.push(ch);
                    self.chars.next();
                } else {
                    break;
                }
            }
            if fraction_digits.is_empty() {
                return Err(MathError::parse(
                    "malformed decimal literal: expected digits after `.`",
                    Some(start),
                ));
            }
        }

        if integer_digits.is_empty() && fraction_digits.is_empty() {
            return Err(MathError::parse("malformed numeric literal", Some(start)));
        }
        if integer_digits.len() + fraction_digits.len() > MAX_LITERAL_DIGITS {
            return Err(MathError::parse(
                format!(
                    "numeric literal has more than {MAX_LITERAL_DIGITS} digits, which is outside the supported resource bound"
                ),
                Some(start),
            ));
        }

        // Reject scientific notation (`1e3`) explicitly: it is outside the
        // subset and would otherwise parse as `1 * e3`.
        if matches!(self.chars.peek(), Some(&(_, 'e' | 'E')))
            && self.number_is_followed_by_exponent()
        {
            return Err(MathError::parse(
                "scientific notation is not supported; write an explicit product such as `2 * 10 ^ 3`",
                Some(start),
            ));
        }

        let integer_text = if integer_digits.is_empty() {
            "0"
        } else {
            &integer_digits
        };
        let numerator = parse_bigint(integer_text, start)?;
        if fraction_digits.is_empty() {
            return Ok(TokenKind::Number(Ratio::from_integer(numerator)));
        }
        let mut all_digits = String::with_capacity(integer_digits.len() + fraction_digits.len());
        all_digits.push_str(if integer_digits.is_empty() {
            "0"
        } else {
            &integer_digits
        });
        all_digits.push_str(&fraction_digits);
        let numerator = parse_bigint(&all_digits, start)?;
        let denominator = BigInt::from(10_u32).pow(fraction_digits.len() as u32);
        Ok(TokenKind::Number(Ratio::new(numerator, denominator)))
    }

    /// Look past the current `e`/`E` to decide whether it introduces an
    /// exponent (`1e3`, `1e-3`) rather than the Euler constant (`2e`).
    fn number_is_followed_by_exponent(&self) -> bool {
        let mut chars = self.chars.clone();
        let _ = chars.next(); // the 'e' / 'E'
        match chars.next().map(|(_, ch)| ch) {
            Some(ch) if ch.is_ascii_digit() => true,
            Some('+' | '-') => {
                matches!(chars.next().map(|(_, ch)| ch), Some(ch) if ch.is_ascii_digit())
            }
            _ => false,
        }
    }

    fn lex_command(&mut self) -> Result<TokenKind, MathError> {
        let start = self.current_offset();
        self.chars.next(); // consume the backslash
        let mut name = String::new();
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_ascii_alphabetic() {
                name.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        match name.as_str() {
            "pi" => Ok(TokenKind::Constant(Constant::Pi)),
            "e" => Ok(TokenKind::Constant(Constant::E)),
            "cdot" | "times" => Ok(TokenKind::Star),
            "frac" => Ok(TokenKind::Frac),
            "sqrt" => Ok(TokenKind::Sqrt),
            "left" => self.lex_left_right(start, '('),
            "right" => self.lex_left_right(start, ')'),
            "" => Err(MathError::unsupported(
                "unsupported LaTeX control symbol; only alphabetic commands such as `\\pi` are supported",
                Some(start),
            )),
            other => Err(MathError::unsupported(
                format!(
                    "unsupported LaTeX command `\\{other}`; the native math subset only supports `\\frac`, `\\sqrt`, `\\cdot`, `\\times`, `\\left(`, `\\right)`, `\\pi` and `\\e`"
                ),
                Some(start),
            )),
        }
    }

    fn lex_left_right(&mut self, start: usize, expected: char) -> Result<TokenKind, MathError> {
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
        match self.chars.next() {
            Some((_, ch)) if ch == expected => Ok(if expected == '(' {
                TokenKind::LParen
            } else {
                TokenKind::RParen
            }),
            _ => Err(MathError::unsupported(
                format!(
                    "only the parenthesis forms `\\left(` and `\\right)` are supported, not `\\{}` followed by another delimiter",
                    if expected == '(' { "left" } else { "right" }
                ),
                Some(start),
            )),
        }
    }
}

fn parse_bigint(digits: &str, position: usize) -> Result<BigInt, MathError> {
    digits
        .parse::<BigInt>()
        .map_err(|_| MathError::parse("numeric literal is too large to represent", Some(position)))
}

fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
    nodes: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        // The lexer always appends an Eof token, so indexing is in bounds.
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    /// Record one constructed AST node, enforcing [`MAX_AST_NODES`].
    fn node(&mut self, expr: Expr) -> Result<Expr, MathError> {
        self.nodes += 1;
        if self.nodes > MAX_AST_NODES {
            return Err(MathError::parse(
                format!(
                    "formula contains more than the supported limit of {MAX_AST_NODES} syntax nodes"
                ),
                Some(self.peek().start),
            ));
        }
        Ok(expr)
    }

    fn enter_depth(&mut self) -> Result<(), MathError> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(MathError::parse(
                format!("formula nesting exceeds the supported limit of {MAX_PARSE_DEPTH} levels"),
                Some(self.peek().start),
            ));
        }
        Ok(())
    }

    fn parse_formula(mut self, source: &str) -> Result<Formula, MathError> {
        if matches!(self.peek().kind, TokenKind::Eof) {
            return Err(MathError::parse("empty formula", Some(0)));
        }
        let left = self.parse_expr()?;
        let kind = if matches!(self.peek().kind, TokenKind::Equals) {
            let equals = self.advance();
            if matches!(self.peek().kind, TokenKind::Eof) {
                return Err(MathError::parse(
                    "expected a right-hand side after `=`",
                    Some(equals.start + 1),
                ));
            }
            let right = self.parse_expr()?;
            if matches!(self.peek().kind, TokenKind::Equals) {
                return Err(MathError::parse(
                    "more than one `=` in a formula; at most one equation is supported and `=` is never assignment",
                    Some(self.peek().start),
                ));
            }
            FormulaKind::Equation { left, right }
        } else {
            FormulaKind::Expression(left)
        };
        if !matches!(self.peek().kind, TokenKind::Eof) {
            return Err(MathError::parse(
                format!("unexpected {} after the formula", self.peek().describe()),
                Some(self.peek().start),
            ));
        }
        Ok(Formula {
            source: source.to_string(),
            kind,
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, MathError> {
        let mut left = self.parse_term()?;
        loop {
            match self.peek().kind {
                TokenKind::Plus => {
                    self.advance();
                    let right = self.parse_term()?;
                    left = self.node(Expr::Add(Box::new(left), Box::new(right)))?;
                }
                TokenKind::Minus => {
                    self.advance();
                    let right = self.parse_term()?;
                    left = self.node(Expr::Sub(Box::new(left), Box::new(right)))?;
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_term(&mut self) -> Result<Expr, MathError> {
        let mut left = self.parse_unary()?;
        loop {
            if matches!(self.peek().kind, TokenKind::Star) {
                self.advance();
                let right = self.parse_unary()?;
                left = self.node(Expr::Mul(Box::new(left), Box::new(right)))?;
            } else if matches!(self.peek().kind, TokenKind::Slash) {
                self.advance();
                let right = self.parse_unary()?;
                left = self.node(Expr::Div(Box::new(left), Box::new(right)))?;
            } else if can_start_primary(&self.peek().kind) {
                if matches!(left, Expr::Number(_))
                    && matches!(self.peek().kind, TokenKind::Number(_))
                {
                    return Err(MathError::parse(
                        "two adjacent number literals are ambiguous; insert an operator",
                        Some(self.peek().start),
                    ));
                }
                // Implicit multiplication: `2x`, `\pi r^2`, `m a`.
                let right = self.parse_unary()?;
                left = self.node(Expr::Mul(Box::new(left), Box::new(right)))?;
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, MathError> {
        self.enter_depth()?;
        let result = self.parse_unary_inner();
        self.depth -= 1;
        result
    }

    fn parse_unary_inner(&mut self) -> Result<Expr, MathError> {
        match self.peek().kind {
            TokenKind::Minus => {
                self.advance();
                let inner = self.parse_unary()?;
                self.node(Expr::Neg(Box::new(inner)))
            }
            TokenKind::Plus => {
                self.advance();
                self.parse_unary()
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, MathError> {
        let base = self.parse_primary()?;
        if matches!(self.peek().kind, TokenKind::Caret) {
            self.advance();
            let exponent = self.parse_unary()?;
            return self.node(Expr::Pow(Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn parse_primary(&mut self) -> Result<Expr, MathError> {
        self.enter_depth()?;
        let result = self.parse_primary_inner();
        self.depth -= 1;
        result
    }

    fn parse_primary_inner(&mut self) -> Result<Expr, MathError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Number(value) => {
                self.advance();
                self.node(Expr::Number(value))
            }
            TokenKind::Ident(name) => {
                self.advance();
                self.node(Expr::Variable(name))
            }
            TokenKind::Constant(constant) => {
                self.advance();
                self.node(Expr::Constant(constant))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect_closing(TokenKind::RParen, "`)`", token.start)?;
                Ok(inner)
            }
            TokenKind::LBrace => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect_closing(TokenKind::RBrace, "`}`", token.start)?;
                Ok(inner)
            }
            TokenKind::Frac => {
                self.advance();
                let numerator = self.parse_braced_argument("\\frac")?;
                let denominator = self.parse_braced_argument("\\frac")?;
                self.node(Expr::Div(Box::new(numerator), Box::new(denominator)))
            }
            TokenKind::Sqrt => {
                self.advance();
                let inner = self.parse_braced_argument("\\sqrt")?;
                self.node(Expr::Sqrt(Box::new(inner)))
            }
            TokenKind::Eof => Err(MathError::parse(
                "expected an expression, found the end of the formula",
                Some(token.start),
            )),
            other => Err(MathError::parse(
                format!(
                    "expected an expression, found {}",
                    Token {
                        kind: other,
                        start: token.start
                    }
                    .describe()
                ),
                Some(token.start),
            )),
        }
    }

    fn parse_braced_argument(&mut self, command: &str) -> Result<Expr, MathError> {
        if !matches!(self.peek().kind, TokenKind::LBrace) {
            return Err(MathError::parse(
                format!("`{command}` requires a braced argument such as `{command}{{x}}`"),
                Some(self.peek().start),
            ));
        }
        let open = self.advance();
        let inner = self.parse_expr()?;
        self.expect_closing(TokenKind::RBrace, "`}`", open.start)?;
        Ok(inner)
    }

    fn expect_closing(
        &mut self,
        expected: TokenKind,
        description: &str,
        open_position: usize,
    ) -> Result<(), MathError> {
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(MathError::parse(
                format!(
                    "expected {description} to close the group opened at byte {open_position}, found {}",
                    self.peek().describe()
                ),
                Some(self.peek().start),
            ))
        }
    }
}

fn can_start_primary(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Number(_)
            | TokenKind::Ident(_)
            | TokenKind::Constant(_)
            | TokenKind::LParen
            | TokenKind::LBrace
            | TokenKind::Frac
            | TokenKind::Sqrt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Formula {
        parse(source).unwrap_or_else(|err| panic!("expected {source:?} to parse: {err}"))
    }

    fn parse_err(source: &str) -> MathError {
        parse(source).unwrap_err()
    }

    #[test]
    fn precedence_matches_standard_arithmetic() {
        let formula = parse_ok("1 + 2 * 3");
        assert_eq!(
            formula.kind,
            FormulaKind::Expression(Expr::Add(
                Box::new(Expr::Number(Ratio::from_integer(BigInt::from(1)))),
                Box::new(Expr::Mul(
                    Box::new(Expr::Number(Ratio::from_integer(BigInt::from(2)))),
                    Box::new(Expr::Number(Ratio::from_integer(BigInt::from(3)))),
                )),
            ))
        );
    }

    #[test]
    fn unary_minus_binds_looser_than_power() {
        // -2^2 is -(2^2), not (-2)^2.
        let formula = parse_ok("-2^2");
        assert!(matches!(
            formula.kind,
            FormulaKind::Expression(Expr::Neg(_))
        ));
    }

    #[test]
    fn power_is_right_associative() {
        let formula = parse_ok("2^3^2");
        let FormulaKind::Expression(Expr::Pow(_, exponent)) = formula.kind else {
            panic!("expected a power");
        };
        assert!(matches!(*exponent, Expr::Pow(_, _)));
    }

    #[test]
    fn implicit_multiplication_joins_adjacent_factors() {
        let formula = parse_ok("\\pi r^2");
        let FormulaKind::Expression(Expr::Mul(left, right)) = formula.kind else {
            panic!("expected multiplication");
        };
        assert!(matches!(*left, Expr::Constant(Constant::Pi)));
        assert!(matches!(*right, Expr::Pow(_, _)));

        // `F = m a` is an equation whose right side is a product.
        let formula = parse_ok("F = m a");
        let FormulaKind::Equation { right, .. } = formula.kind else {
            panic!("expected an equation");
        };
        assert!(matches!(right, Expr::Mul(_, _)));
    }

    #[test]
    fn left_and_right_parentheses_are_accepted() {
        assert_eq!(
            parse_ok("\\left(1 + 2\\right) * 3").kind,
            parse_ok("(1 + 2) * 3").kind
        );
    }

    #[test]
    fn frac_and_sqrt_become_dedicated_nodes() {
        let formula = parse_ok("\\frac{1}{2} + \\sqrt{4}");
        let FormulaKind::Expression(Expr::Add(left, right)) = formula.kind else {
            panic!("expected addition");
        };
        assert!(matches!(*left, Expr::Div(_, _)));
        assert!(matches!(*right, Expr::Sqrt(_)));
    }

    #[test]
    fn single_equals_is_an_equation() {
        let formula = parse_ok("A = \\pi r^2");
        assert!(formula.is_equation());
        assert_eq!(formula.variables().len(), 2);
    }

    #[test]
    fn more_than_one_equals_is_a_parse_error() {
        let err = parse_err("a = b = c");
        assert_eq!(err.kind, crate::error::MathErrorKind::Parse);
        assert!(err.message.contains("more than one"));
    }

    #[test]
    fn empty_and_malformed_input_is_a_parse_error() {
        assert_eq!(parse_err("").kind, crate::error::MathErrorKind::Parse);
        for source in ["1 +", "(1 + 2", "\\frac{1}{2", "{1 + 2", "1 + 2)", "= 3"] {
            let err = parse_err(source);
            assert_eq!(
                err.kind,
                crate::error::MathErrorKind::Parse,
                "source {source:?}"
            );
            assert!(err.position.is_some(), "source {source:?}");
        }
    }

    #[test]
    fn unsupported_commands_are_structured() {
        for source in [
            "\\begin{matrix}1&2\\end{matrix}",
            "\\int_0^1 x dx",
            "\\sin(x)",
            "\\sum_{i=1}^n i",
            "sin(1)",
        ] {
            let err = parse_err(source);
            assert_eq!(
                err.kind,
                crate::error::MathErrorKind::Unsupported,
                "source {source:?}"
            );
        }
    }

    #[test]
    fn unknown_characters_are_structured() {
        let err = parse_err("1, 2");
        assert_eq!(err.kind, crate::error::MathErrorKind::Unsupported);
        assert!(err.message.contains("unsupported character"));
    }

    #[test]
    fn deep_nesting_is_rejected_without_stack_overflow() {
        let source = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        let err = parse_err(&source);
        assert_eq!(err.kind, crate::error::MathErrorKind::Parse);
        assert!(err.message.contains("nesting"));
    }

    #[test]
    fn excessive_node_count_is_rejected() {
        // A long left-associative chain is cheap to parse iteratively but would
        // build a very deep tree; the node budget rejects it first.
        let source = format!("{}1", "1 + ".repeat(MAX_AST_NODES));
        let err = parse_err(&source);
        assert_eq!(err.kind, crate::error::MathErrorKind::Parse);
        assert!(err.message.contains("syntax nodes"), "{}", err.message);
    }

    #[test]
    fn very_long_chain_is_rejected_without_stack_overflow() {
        let source = format!("{}1", "1 + ".repeat(200_000));
        let err = parse_err(&source);
        assert_eq!(err.kind, crate::error::MathErrorKind::Parse);
    }

    #[test]
    fn numeric_literal_bounds_are_enforced() {
        let source = "9".repeat(MAX_LITERAL_DIGITS + 1);
        let err = parse_err(&source);
        assert_eq!(err.kind, crate::error::MathErrorKind::Parse);
        assert!(err.message.contains("digits"));
    }
}
