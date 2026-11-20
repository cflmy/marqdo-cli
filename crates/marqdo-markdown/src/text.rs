//! Inert text extraction and strict executable payload extraction.
//!
//! Two different operations live here on purpose:
//!
//! * [`plain_text`] renders inert content (heading names, table labels); it
//!   never decides that something is executable.
//! * [`executable_payload`] extracts the payload of an executable `Strong` or
//!   `Emph` node. It accepts only `Text` and `Code` children and fails on any
//!   other inline structure: an executable node with an unexpected shape is a
//!   hard compile error, never a silent fallback to prose.

use unicode_normalization::UnicodeNormalization;

use marqdo_span::Span;

use crate::ast::{MdKind, MdNode};

/// Why a payload could not be extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadError {
    /// Span of the offending node.
    pub span: Span,
    /// Human readable reason.
    pub reason: String,
}

impl PayloadError {
    fn new(span: Span, reason: impl Into<String>) -> Self {
        Self {
            span,
            reason: reason.into(),
        }
    }
}

/// Extract the strict payload of an executable inline node.
///
/// Only `Text` and `Code` children are allowed; their literals are concatenated
/// exactly as written (a code span keeps its payload verbatim). Nested
/// `Strong`/`Emph`/`Link` and line breaks are rejected so that one AST node can
/// never be executed twice or hide control content.
pub fn executable_payload(node: &MdNode) -> Result<String, PayloadError> {
    let mut payload = String::new();
    for child in &node.children {
        match &child.kind {
            MdKind::Text => payload.push_str(child.literal.as_deref().unwrap_or_default()),
            MdKind::Code => payload.push_str(child.literal.as_deref().unwrap_or_default()),
            other => {
                return Err(PayloadError::new(
                    child.span,
                    format!(
                        "`{}` is not a valid payload inside an executable node",
                        other.tag()
                    ),
                ));
            }
        }
    }
    Ok(payload)
}

/// Render inert content to plain text.
///
/// Used for heading names and other inert labels. Images use their alt text,
/// links use their label, code and math use their literal source, and line
/// breaks become a single space.
pub fn plain_text(node: &MdNode) -> String {
    let mut out = String::new();
    collect_plain_text(node, &mut out);
    out
}

fn collect_plain_text(node: &MdNode, out: &mut String) {
    match &node.kind {
        MdKind::Text | MdKind::Code | MdKind::Math { .. } | MdKind::MathBlock => {
            out.push_str(node.literal.as_deref().unwrap_or_default());
        }
        MdKind::SoftBreak | MdKind::LineBreak => out.push(' '),
        MdKind::HtmlBlock | MdKind::HtmlInline | MdKind::FrontMatter => {
            // Raw HTML and front matter are not part of visible prose.
        }
        _ => {
            for child in &node.children {
                collect_plain_text(child, out);
            }
        }
    }
}

/// Normalise a heading name: NFC, trim, and collapse internal whitespace runs
/// to a single space. Returns `None` when nothing visible remains.
pub fn normalize_name(text: &str) -> Option<String> {
    let normalized: String = text.nfc().collect();
    let mut collapsed = String::new();
    let mut pending_space = false;
    for ch in normalized.trim().chars() {
        if ch.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            pending_space = false;
            collapsed.push(ch);
        }
    }
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: MdKind, literal: Option<&str>, children: Vec<MdNode>) -> MdNode {
        MdNode {
            kind,
            span: Span::new(0, 0),
            literal: literal.map(str::to_string),
            children,
        }
    }

    #[test]
    fn payload_accepts_text_and_code() {
        let strong = node(
            MdKind::Strong,
            None,
            vec![
                node(MdKind::Text, Some("面积 = "), vec![]),
                node(MdKind::Code, Some("计算(r=1)"), vec![]),
            ],
        );
        assert_eq!(executable_payload(&strong).unwrap(), "面积 = 计算(r=1)");
    }

    #[test]
    fn payload_rejects_nested_markup() {
        let strong = node(
            MdKind::Strong,
            None,
            vec![node(
                MdKind::Emph,
                None,
                vec![node(MdKind::Text, Some("x"), vec![])],
            )],
        );
        let error = executable_payload(&strong).unwrap_err();
        assert!(error.reason.contains("emph"));
    }

    #[test]
    fn plain_text_uses_alt_and_label() {
        let image = node(
            MdKind::Image {
                url: "a.png".into(),
                title: String::new(),
            },
            None,
            vec![node(MdKind::Text, Some("示意图"), vec![])],
        );
        let heading = node(
            MdKind::Heading {
                level: 1,
                setext: false,
            },
            None,
            vec![
                image,
                node(MdKind::SoftBreak, None, vec![]),
                node(MdKind::Text, Some("名字"), vec![]),
            ],
        );
        assert_eq!(plain_text(&heading), "示意图 名字");
    }

    #[test]
    fn normalize_collapses_whitespace_and_nfc() {
        assert_eq!(normalize_name("  圆  面积 ").as_deref(), Some("圆 面积"));
        // Decomposed e + combining acute becomes NFC single code point.
        assert_eq!(normalize_name("e\u{301}").as_deref(), Some("é"));
        assert_eq!(normalize_name("   "), None);
    }
}
