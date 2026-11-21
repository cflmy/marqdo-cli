//! Optional explicit function signatures.
//!
//! `` `(r: Number, precision: Int = 2)` `` as the first paragraph of a
//! function body declares the public parameter list. It is a *declaration*:
//! it does not execute. Without it, parameters are inferred from free reads.

use marqdo_expression::{parse_expression, Expr, Statement, StatementKind};
use marqdo_ir::{Param, SignatureKind};
use marqdo_markdown::{MarkdownDocument, MdKind, MdNode};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};
use unicode_ident::{is_xid_continue, is_xid_start};

use crate::payload::PayloadSegments;

/// Parsed signature declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    /// Parameters in declaration order.
    pub params: Vec<Param>,
    /// Span of the declaration code span.
    pub span: Span,
    /// Diagnostics raised while parsing the declaration.
    pub diagnostics: Vec<Diagnostic>,
}

impl Signature {
    /// Whether the declaration had errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Detect and parse a signature paragraph.
///
/// A paragraph qualifies only when it is the first block in the function body
/// (ignoring inert level>=3 section headings) and consists of exactly one
/// inline code span whose payload starts with `(`. Any other first paragraph is
/// prose and inference is used instead.
pub fn detect(_doc: &MarkdownDocument, blocks: &[&MdNode]) -> Option<(usize, Signature)> {
    for (index, block) in blocks.iter().enumerate() {
        match &block.kind {
            MdKind::Heading { level, .. } if *level >= 3 => continue,
            MdKind::Paragraph => {
                if block.children.len() != 1 {
                    continue;
                }
                let carrier = &block.children[0];
                let (source, span, segments) = match &carrier.kind {
                    MdKind::Code { ticks } => {
                        let literal = carrier.literal.clone().unwrap_or_default();
                        let inner = Span::new(
                            carrier.span.start.saturating_add(*ticks),
                            carrier.span.end.saturating_sub(*ticks),
                        );
                        (
                            literal.clone(),
                            carrier.span,
                            PayloadSegments::single(&literal, inner),
                        )
                    }
                    MdKind::Strong | MdKind::Emph => match PayloadSegments::of(carrier) {
                        Ok((source, segments)) => (source, carrier.span, segments),
                        Err(_) => continue,
                    },
                    _ => continue,
                };
                if !source.trim_start().starts_with("params(") {
                    return None;
                }
                let mut signature = parse(&source, span);
                for param in &mut signature.params {
                    if let Some(default) = &mut param.default {
                        segments.rewrite_expr(default);
                    }
                }
                return Some((index, signature));
            }
            MdKind::List { .. } | MdKind::BlockQuote | MdKind::Item { .. } => continue,
            _ => return None,
        }
    }
    None
}

/// Parse signature payload text (without the code span delimiters).
pub fn parse(source: &str, span: Span) -> Signature {
    let mut diagnostics = Vec::new();
    let trimmed = source.trim();
    let is_params = trimmed.starts_with("params(");
    let syntax_code = if is_params {
        codes::SEM_SYNTAX
    } else {
        codes::EXPR_PARSE
    };
    let inner = if let Some(rest) = trimmed.strip_prefix("params(") {
        match rest.strip_suffix(')') {
            Some(inner) => inner,
            None => {
                diagnostics.push(Diagnostic::error(
                    codes::SEM_SYNTAX,
                    Some(span),
                    "function signature must be closed by `)`",
                ));
                rest
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix('(') {
        // Plugin/ABI descriptors keep their existing parenthesised signature
        // spelling; language payloads are detected only as `params(...)`.
        rest.strip_suffix(')').unwrap_or(rest)
    } else {
        diagnostics.push(Diagnostic::error(
            syntax_code,
            Some(span),
            "function signature must be `params(...)`, e.g. `params(r: Number)`",
        ));
        trimmed
    };

    let mut params: Vec<Param> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for (entry_start, entry_end) in split_top_level(inner, ',') {
        let entry = &inner[entry_start..entry_end];
        if entry.trim().is_empty() {
            if !inner.trim().is_empty() {
                diagnostics.push(Diagnostic::error(
                    syntax_code,
                    Some(span),
                    "empty parameter in function signature",
                ));
            }
            cursor = entry_end;
            continue;
        }
        let entry_offset = source.find(entry).unwrap_or(0);
        match parse_entry(entry, span) {
            Ok(mut param) => {
                param.span = Span::new(
                    span.start + entry_offset.min(span.len()),
                    span.start + (entry_offset + entry.len()).min(span.len()),
                );
                if seen.contains(&param.name) {
                    diagnostics.push(Diagnostic::error(
                        codes::EXPR_PARSE,
                        Some(param.span),
                        format!("duplicate parameter `{}`", param.name),
                    ));
                } else {
                    seen.push(param.name.clone());
                    params.push(param);
                }
            }
            Err(message) => diagnostics.push(Diagnostic::error(
                syntax_code,
                Some(span),
                format!("invalid function signature: {message}"),
            )),
        }
        cursor = entry_end;
    }
    let _ = cursor;
    Signature {
        params,
        span,
        diagnostics,
    }
}

fn parse_entry(entry: &str, span: Span) -> Result<Param, String> {
    let entry = entry.trim();
    // Find `=` at top level (not part of `==`).
    let equals = find_top_level(entry, '=');
    let (lhs, default) = match equals {
        Some(index)
            if !entry[index + 1..].starts_with('=')
                && (index == 0 || entry.as_bytes()[index - 1] != b'=') =>
        {
            let default_text = entry[index + 1..].trim();
            if default_text.is_empty() {
                return Err("default value expression is empty".to_string());
            }
            let expr = parse_expression(default_text)
                .map_err(|error| format!("invalid default value: {}", error.message))?;
            if !is_const_expr(&expr) {
                return Err("default values must be compile-time constants".to_string());
            }
            (&entry[..index], Some(expr))
        }
        _ => (entry, None),
    };
    let colon = find_top_level(lhs, ':');
    let (name_text, type_annotation) = match colon {
        Some(index) => (
            lhs[..index].trim(),
            Some(lhs[index + 1..].trim().to_string()),
        ),
        None => (lhs.trim(), None),
    };
    if let Some(annotation) = &type_annotation {
        if annotation.is_empty() {
            return Err("type annotation is empty".to_string());
        }
    }
    validate_ident(name_text)?;
    Ok(Param {
        name: name_text.to_string(),
        // Refined by the caller with the exact source span.
        span,
        type_annotation,
        default,
    })
}

fn validate_ident(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("parameter name is empty".to_string());
    };
    if !(first == '_' || is_xid_start(first)) {
        return Err(format!("`{name}` is not a valid parameter name"));
    }
    for ch in chars {
        if !(ch == '_' || is_xid_continue(ch)) {
            return Err(format!("`{name}` is not a valid parameter name"));
        }
    }
    Ok(())
}

/// Compile-time constant check for signature defaults.
fn is_const_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => true,
        Expr::Unary { operand, .. } => is_const_expr(operand),
        Expr::List(items) => items.iter().all(is_const_expr),
        Expr::Record(entries) => entries.iter().all(|entry| is_const_expr(&entry.value)),
        _ => false,
    }
}

/// Split `text` on a separator at bracket/string depth zero, returning byte
/// ranges (without the separator).
fn split_top_level(text: &str, separator: char) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == separator && depth == 0 => {
                ranges.push((start, index));
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    ranges.push((start, text.len()));
    ranges
}

fn find_top_level(text: &str, needle: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == needle && depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

/// Build a statement-less const check helper used by the mapper for defaults.
pub fn const_default_ok(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Assign { value, .. } => is_const_expr(value),
        StatementKind::Expr(expr) => is_const_expr(expr),
    }
}

/// Convenience: parameter kind for functions with a signature.
pub const KIND: SignatureKind = SignatureKind::Explicit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_signature_with_default() {
        let signature = parse("params(r: Number, precision: Int = 2)", Span::new(0, 38));
        assert!(!signature.has_errors(), "{:?}", signature.diagnostics);
        assert_eq!(signature.params.len(), 2);
        assert_eq!(signature.params[0].name, "r");
        assert_eq!(
            signature.params[0].type_annotation.as_deref(),
            Some("Number")
        );
        assert_eq!(signature.params[1].name, "precision");
        assert!(signature.params[1].default.is_some());
    }

    #[test]
    fn parses_untyped_and_empty() {
        let signature = parse("params()", Span::new(0, 8));
        assert!(signature.params.is_empty());
        assert!(!signature.has_errors());
        let signature = parse("params(n)", Span::new(0, 9));
        assert_eq!(signature.params[0].name, "n");
    }

    #[test]
    fn rejects_duplicates_and_bad_defaults() {
        let duplicate = parse("params(a, a)", Span::new(0, 12));
        assert!(duplicate.has_errors());
        let bad_default = parse("params(a = f())", Span::new(0, 16));
        assert!(bad_default.has_errors());
        let bad_name = parse("params(1x)", Span::new(0, 10));
        assert!(bad_name.has_errors());
    }

    #[test]
    fn split_respects_nesting_and_strings() {
        let parts = split_top_level("a, f(b, c), d", ',');
        assert_eq!(parts.len(), 3);
    }
}
