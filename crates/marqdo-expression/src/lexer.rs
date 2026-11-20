//! Hand written lexer for the Marqdo expression grammar v1.
//!
//! This module is an implementation detail that is exposed only to make
//! diagnostics and tests easier; the contractual API of the crate lives in
//! [`crate::parse_expression`], [`crate::parse_statement`] and
//! [`crate::parse_arguments`] plus the [`crate::ast`] types.
//!
//! Lexical rules (see `spec/expression.ebnf`):
//!
//! * Unicode whitespace separates tokens (newlines included).
//! * Identifiers use UAX #31 `XID_Start` / `XID_Continue`.
//! * Integers are arbitrary-precision decimal digit runs.
//! * Floats require digits on both sides of `.` and may carry an exponent.
//! * Text literals are double quoted with the escapes `\\`, `\"`, `\n`, `\t`,
//!   `\r` and `\u{...}`.
//! * There are no comments in v1.

use crate::ParseError;
use marqdo_span::Span;

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// Decimal integer literal, kept as raw digits (arbitrary precision).
    Int(String),
    /// Float literal, kept as raw source text (parsed to `f64` by the parser).
    Float(String),
    /// Text literal whose escapes have already been decoded.
    Text(String),
    /// Identifier text (never a keyword).
    Ident(String),
    /// `true`
    True,
    /// `false`
    False,
    /// `None`
    NoneKw,
    /// `and`
    And,
    /// `or`
    Or,
    /// `not`
    Not,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `.`
    Dot,
    /// `=` (assignment)
    Assign,
    /// `==`
    EqEq,
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
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `**`
    StarStar,
    /// `/`
    Slash,
    /// `//`
    SlashSlash,
    /// `%`
    Percent,
    /// End of input.
    Eof,
}

/// A token with its byte span in the payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Byte span of the token.
    pub span: Span,
}

/// Tokenize a payload string.
///
/// The returned vector always ends with exactly one [`TokenKind::Eof`] token
/// whose span is the empty span at the end of the input.
pub fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    Lexer::new(source).tokenize()
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn error(&self, start: usize, message: impl Into<String>) -> ParseError {
        ParseError::new(Span::new(start, self.pos), message)
    }

    fn tokenize(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let start = self.pos;
            let Some(c) = self.peek() else {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::empty(self.source.len()),
                });
                return Ok(tokens);
            };
            let kind = match c {
                '"' => TokenKind::Text(self.scan_text()?),
                '0'..='9' => self.scan_number()?,
                _ if unicode_ident::is_xid_start(c) => self.scan_identifier(),
                '(' => {
                    self.bump();
                    TokenKind::LParen
                }
                ')' => {
                    self.bump();
                    TokenKind::RParen
                }
                '[' => {
                    self.bump();
                    TokenKind::LBracket
                }
                ']' => {
                    self.bump();
                    TokenKind::RBracket
                }
                '{' => {
                    self.bump();
                    TokenKind::LBrace
                }
                '}' => {
                    self.bump();
                    TokenKind::RBrace
                }
                ',' => {
                    self.bump();
                    TokenKind::Comma
                }
                ':' => {
                    self.bump();
                    TokenKind::Colon
                }
                '.' => {
                    self.bump();
                    if matches!(self.peek(), Some(next) if next.is_ascii_digit()) {
                        return Err(self.error(
                            start,
                            "invalid float literal: a decimal point must be preceded by a digit",
                        ));
                    }
                    TokenKind::Dot
                }
                '=' => {
                    self.bump();
                    if matches!(self.peek(), Some('=')) {
                        self.bump();
                        TokenKind::EqEq
                    } else {
                        TokenKind::Assign
                    }
                }
                '!' => {
                    self.bump();
                    if matches!(self.peek(), Some('=')) {
                        self.bump();
                        TokenKind::Ne
                    } else {
                        return Err(self.error(
                            start,
                            "unexpected character `!`; logical negation is written `not`",
                        ));
                    }
                }
                '<' => {
                    self.bump();
                    if matches!(self.peek(), Some('=')) {
                        self.bump();
                        TokenKind::Le
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    self.bump();
                    if matches!(self.peek(), Some('=')) {
                        self.bump();
                        TokenKind::Ge
                    } else {
                        TokenKind::Gt
                    }
                }
                '+' => {
                    self.bump();
                    TokenKind::Plus
                }
                '-' => {
                    self.bump();
                    TokenKind::Minus
                }
                '*' => {
                    self.bump();
                    if matches!(self.peek(), Some('*')) {
                        self.bump();
                        TokenKind::StarStar
                    } else {
                        TokenKind::Star
                    }
                }
                '/' => {
                    self.bump();
                    if matches!(self.peek(), Some('/')) {
                        self.bump();
                        TokenKind::SlashSlash
                    } else {
                        TokenKind::Slash
                    }
                }
                '%' => {
                    self.bump();
                    TokenKind::Percent
                }
                '#' => {
                    self.bump();
                    return Err(self.error(
                        start,
                        "unexpected character `#`; comments are not supported in v1",
                    ));
                }
                other => {
                    self.bump();
                    return Err(self.error(start, format!("unexpected character {:?}", other)));
                }
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, self.pos),
            });
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn scan_identifier(&mut self) -> TokenKind {
        let start = self.pos;
        // The caller guarantees `is_xid_start` for the first character.
        self.bump();
        while let Some(c) = self.peek() {
            if unicode_ident::is_xid_continue(c) {
                self.bump();
            } else {
                break;
            }
        }
        let text = &self.source[start..self.pos];
        match text {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "None" => TokenKind::NoneKw,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            _ => TokenKind::Ident(text.to_owned()),
        }
    }

    fn scan_number(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.bump();
        }
        let mut is_float = false;
        if matches!(self.peek(), Some('.')) {
            self.bump();
            if !matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                return Err(self.error(
                    start,
                    "invalid float literal: expected at least one digit after `.`",
                ));
            }
            is_float = true;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            if !is_float {
                return Err(self.error(
                    start,
                    "invalid float literal: an exponent requires digits on both sides of `.`",
                ));
            }
            self.bump();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                return Err(self.error(
                    start,
                    "invalid float literal: expected at least one exponent digit",
                ));
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
            }
        }
        if let Some(c) = self.peek() {
            if c == '.' || unicode_ident::is_xid_continue(c) {
                return Err(self.error(
                    start,
                    format!("invalid numeric literal: unexpected `{c}` immediately after a number"),
                ));
            }
        }
        let raw = self.source[start..self.pos].to_owned();
        Ok(if is_float {
            TokenKind::Float(raw)
        } else {
            TokenKind::Int(raw)
        })
    }

    fn scan_text(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        self.bump(); // opening quote
        let mut out = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(ParseError::new(
                    Span::new(start, self.source.len()),
                    "unterminated text literal: expected a closing `\"`",
                ));
            };
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let escape_start = self.pos - 1;
                    match self.bump() {
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some('u') => self.scan_unicode_escape(escape_start, &mut out)?,
                        Some(other) => {
                            return Err(self.error(
                                escape_start,
                                format!(
                                    "invalid escape sequence `\\{other}`; supported escapes are \
                                     `\\\\`, `\\\"`, `\\n`, `\\t`, `\\r` and `\\u{{...}}`"
                                ),
                            ));
                        }
                        None => {
                            return Err(ParseError::new(
                                Span::new(start, self.source.len()),
                                "unterminated text literal: expected a closing `\"`",
                            ));
                        }
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn scan_unicode_escape(
        &mut self,
        escape_start: usize,
        out: &mut String,
    ) -> Result<(), ParseError> {
        if !matches!(self.peek(), Some('{')) {
            return Err(self.error(
                escape_start,
                "invalid unicode escape: expected `{` after `\\u`",
            ));
        }
        self.bump();
        let digits_start = self.pos;
        let mut value: u32 = 0;
        let mut overflow = false;
        while let Some(c) = self.peek() {
            let Some(digit) = c.to_digit(16) else {
                break;
            };
            self.bump();
            value = match value.checked_mul(16).and_then(|v| v.checked_add(digit)) {
                Some(v) => v,
                None => {
                    overflow = true;
                    0
                }
            };
        }
        if self.pos == digits_start {
            return Err(self.error(
                escape_start,
                "invalid unicode escape: expected at least one hexadecimal digit",
            ));
        }
        if !matches!(self.peek(), Some('}')) {
            return Err(self.error(
                escape_start,
                "invalid unicode escape: expected a closing `}`",
            ));
        }
        self.bump();
        if overflow {
            return Err(self.error(
                escape_start,
                "invalid unicode escape: code point is out of range",
            ));
        }
        match char::from_u32(value) {
            Some(c) => {
                out.push(c);
                Ok(())
            }
            None => Err(self.error(
                escape_start,
                format!("invalid unicode escape: U+{value:04X} is not a Unicode scalar value"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn tokenizes_operators_and_punctuation() {
        assert_eq!(
            kinds("== != < <= > >= = + - * ** / // % ( ) [ ] { } , : ."),
            vec![
                TokenKind::EqEq,
                TokenKind::Ne,
                TokenKind::Lt,
                TokenKind::Le,
                TokenKind::Gt,
                TokenKind::Ge,
                TokenKind::Assign,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::StarStar,
                TokenKind::Slash,
                TokenKind::SlashSlash,
                TokenKind::Percent,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::Comma,
                TokenKind::Colon,
                TokenKind::Dot,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_keywords_separately_from_identifiers() {
        assert_eq!(
            kinds("true false None and or not truex None_ 真实"),
            vec![
                TokenKind::True,
                TokenKind::False,
                TokenKind::NoneKw,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::Not,
                TokenKind::Ident("truex".into()),
                TokenKind::Ident("None_".into()),
                TokenKind::Ident("真实".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn token_remembers_multibyte_spans() {
        let tokens = tokenize("张三 + 1").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("张三".into()));
        assert_eq!(tokens[0].span, Span::new(0, 6));
        assert_eq!(tokens[1].span, Span::new(7, 8));
        assert_eq!(tokens[2].kind, TokenKind::Int("1".into()));
        assert_eq!(tokens[2].span, Span::new(9, 10));
        assert_eq!(tokens[3].span, Span::empty(10));
    }

    #[test]
    fn unicode_whitespace_is_skipped() {
        assert_eq!(
            kinds("1\u{00a0}+\u{2003}2\n"),
            vec![
                TokenKind::Int("1".into()),
                TokenKind::Plus,
                TokenKind::Int("2".into()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn number_lexing_rejects_invalid_forms() {
        for bad in [".5", "1.", "1e5", "1.2.3", "1.5e", "1.5e+", "1a", "1_2"] {
            let err = tokenize(bad).unwrap_err();
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
        assert_eq!(
            kinds("1.5e+3 0.0 007 1.25E-2"),
            vec![
                TokenKind::Float("1.5e+3".into()),
                TokenKind::Float("0.0".into()),
                TokenKind::Int("007".into()),
                TokenKind::Float("1.25E-2".into()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn text_escapes_decode() {
        let tokens = tokenize(r#""a\\b\"c\n\t\r\u{4e2d}""#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Text("a\\b\"c\n\t\r中".into()));
    }

    #[test]
    fn bad_text_lexing_reports_errors() {
        for bad in [
            r#""unterminated"#,
            r#""bad \q escape""#,
            r#""bad \u{} escape""#,
            r#""bad \u{D800} escape""#,
            r#""bad \u{110000} escape""#,
            r#""bad \u12 escape""#,
        ] {
            let err = tokenize(bad).unwrap_err();
            assert_eq!(err.code, "expr.parse", "input {bad:?}");
        }
    }

    #[test]
    fn comments_are_not_supported() {
        let err = tokenize("# comment").unwrap_err();
        assert!(err.message.contains("comments"));
        let err = tokenize("1 # 2").unwrap_err();
        assert!(err.message.contains("comments"));
    }
}
