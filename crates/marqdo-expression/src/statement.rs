//! The v2 payload statement grammar (RFC-LANG-0002).
//!
//! One Markdown carrier holds exactly one payload. This module is the only
//! place that decides whether that payload is a control statement
//! (`if`/`else if`/`else`/`while`/`end`/`return`), a `params(...)` declaration, or an
//! ordinary assignment/expression. The frontend still owns pairing the control
//! words into blocks; this module owns their lexical recognition and the
//! expression grammar they embed.
//!
//! Keyword matching is by complete token: `ifdef(x)` and `returning()` are
//! ordinary calls, while a bare `if` used as an identifier is a reserved-word
//! error. `if`/`while`/`return` require whitespace before their expression and
//! the whole payload must be consumed.

use crate::ast::{Expr, Statement};
use crate::lexer::{self, TokenKind};
use crate::{parse_expression, parse_statement, ParseError};
use marqdo_span::Span;

/// A payload parsed by the v2 statement grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadStatement {
    /// `if <expr>`
    If(Expr),
    /// `else if <expr>`
    ElseIf(Expr),
    /// `else` (no expression)
    Else,
    /// `while <expr>`
    While(Expr),
    /// `end` (no expression)
    End,
    /// `return <expr>`
    Return(Expr),
    /// `params(...)`; the parameter list itself is validated by the frontend
    /// signature parser, which owns IR parameter types and defaults.
    Params,
    /// An ordinary assignment or expression statement.
    Ordinary(Statement),
}

/// Parse one complete executable payload under the v2 statement grammar.
///
/// The input is the concatenated carrier text, already trimmed or not; leading
/// and trailing whitespace is ignored. Expressions keep byte spans relative to
/// the original payload, including the keyword prefix, so diagnostics and IR
/// source maps point inside the file after the frontend rewrites them.
pub fn parse_payload_statement(source: &str) -> Result<PayloadStatement, ParseError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new(Span::empty(0), "empty executable payload"));
    }

    // `params(...)` is a declaration whose interior is validated by
    // `signature::parse`; recognize the prefix before tokenizing so malformed
    // parameter text (for example `params(1x: Int)`) is reported once by the
    // signature parser instead of as a lexer failure.
    if trimmed.starts_with("params(") {
        return Ok(PayloadStatement::Params);
    }
    if trimmed == "params" || trimmed.starts_with("params ") || trimmed.starts_with("params\t") {
        return Err(ParseError::new(
            Span::new(0, "params".len()),
            "malformed params declaration; write `params(name: Type, ...)`",
        ));
    }

    let tokens = lexer::tokenize(trimmed)?;
    let first = tokens.first().expect("tokenize always returns EOF");
    match first.kind {
        TokenKind::If => {
            let expr = control_expression(trimmed, first.span, "if")?;
            Ok(PayloadStatement::If(expr))
        }
        TokenKind::While => {
            let expr = control_expression(trimmed, first.span, "while")?;
            Ok(PayloadStatement::While(expr))
        }
        TokenKind::Return => {
            let expr = control_expression(trimmed, first.span, "return")?;
            Ok(PayloadStatement::Return(expr))
        }
        TokenKind::Else => {
            if matches!(tokens[1].kind, TokenKind::If) {
                let keyword = Span::new(first.span.start, tokens[1].span.end);
                let expr = control_expression(trimmed, keyword, "else if")?;
                return Ok(PayloadStatement::ElseIf(expr));
            }
            require_only_keyword(&tokens, "else")?;
            Ok(PayloadStatement::Else)
        }
        TokenKind::End => {
            require_only_keyword(&tokens, "end")?;
            Ok(PayloadStatement::End)
        }
        TokenKind::Params => Err(ParseError::new(
            first.span,
            "malformed params declaration; write `params(name: Type, ...)`",
        )),
        _ => parse_statement(trimmed).map(PayloadStatement::Ordinary),
    }
}

fn control_expression(source: &str, keyword: Span, name: &str) -> Result<Expr, ParseError> {
    let rest = &source[keyword.end..];
    if rest.is_empty() || !rest.starts_with(char::is_whitespace) {
        return Err(ParseError::new(
            keyword,
            format!("`{name}` must be followed by whitespace and an expression"),
        ));
    }
    // Pad the expression so its AST spans stay payload-relative.
    let padded = format!("{}{}", " ".repeat(keyword.end), rest);
    parse_expression(&padded)
}

fn require_only_keyword(tokens: &[crate::lexer::Token], name: &str) -> Result<(), ParseError> {
    let mut non_eof = tokens
        .iter()
        .filter(|token| !matches!(token.kind, TokenKind::Eof));
    let _keyword = non_eof.next();
    if let Some(extra) = non_eof.next() {
        return Err(ParseError::new(
            extra.span,
            format!("`{name}` takes no expression"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr_of(source: &str) -> Expr {
        match parse_payload_statement(source).expect("valid payload") {
            PayloadStatement::If(expr)
            | PayloadStatement::ElseIf(expr)
            | PayloadStatement::While(expr)
            | PayloadStatement::Return(expr) => expr,
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn parses_control_statements_with_payload_relative_spans() {
        for keyword in ["if", "else if", "while", "return"] {
            let source = format!("{keyword} x > 0");
            let expr = expr_of(&source);
            assert_eq!(expr.span().start, keyword.len() + 1);
        }
    }

    #[test]
    fn else_and_end_are_exact_payloads() {
        assert_eq!(
            parse_payload_statement("else").unwrap(),
            PayloadStatement::Else
        );
        assert_eq!(
            parse_payload_statement("end").unwrap(),
            PayloadStatement::End
        );
        assert!(parse_payload_statement("else x").is_err());
        assert!(parse_payload_statement("end if").is_err());
    }

    #[test]
    fn keywords_require_whitespace_and_an_expression() {
        for source in [
            "if",
            "else if",
            "while",
            "return",
            "if(x)",
            "else if(x)",
            "while(1)",
            "return(x)",
        ] {
            assert!(
                parse_payload_statement(source).is_err(),
                "{source} must be rejected"
            );
        }
    }

    #[test]
    fn control_expressions_must_consume_the_payload() {
        assert!(parse_payload_statement("if x > 0:").is_err());
        assert!(parse_payload_statement("else if x > 0:").is_err());
        assert!(parse_payload_statement("if x y").is_err());
    }

    #[test]
    fn keyword_prefixes_are_ordinary_identifiers() {
        for source in ["ifdef(x)", "returning(x)", "endpoint(x)", "while_count(x)"] {
            match parse_payload_statement(source).unwrap() {
                PayloadStatement::Ordinary(statement) => {
                    assert!(matches!(statement.kind, crate::StatementKind::Expr(_)));
                }
                other => panic!("{source} parsed as {other:?}"),
            }
        }
    }

    #[test]
    fn bare_keywords_are_reserved_in_expressions() {
        for source in ["x = if", "x = return", "[else]", "{while: 1}"] {
            assert!(parse_payload_statement(source).is_err(), "{source}");
        }
    }

    #[test]
    fn params_are_recognized_before_signature_validation() {
        assert_eq!(
            parse_payload_statement("params(n: Int)").unwrap(),
            PayloadStatement::Params
        );
        // Interior validation belongs to `signature::parse`; the statement
        // grammar only needs to recognize the declaration payload.
        assert_eq!(
            parse_payload_statement("params(1x: Int)").unwrap(),
            PayloadStatement::Params
        );
        assert!(parse_payload_statement("params").is_err());
        assert!(parse_payload_statement("params (x)").is_err());
        assert!(parse_payload_statement("params\t(x)").is_err());
    }

    #[test]
    fn ordinary_statements_stay_ordinary() {
        for source in ["x = 1", "print(x)", "m[0] = 1", "x = 1; return x"] {
            let parsed = parse_payload_statement(source);
            if source.contains(';') {
                assert!(parsed.is_err(), "{source}");
            } else {
                assert!(matches!(parsed.unwrap(), PayloadStatement::Ordinary(_)));
            }
        }
    }

    #[test]
    fn expressions_cannot_use_statement_keywords_as_identifiers() {
        assert!(crate::parse_expression("if").is_err());
        assert!(crate::parse_expression("return").is_err());
    }
}
