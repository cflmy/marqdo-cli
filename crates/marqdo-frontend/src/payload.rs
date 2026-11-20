//! Executable payload extraction with a payload → file span map.
//!
//! A payload is the concatenation of the direct `text` and `code` children of
//! an executable node. The expression parser sees only that concatenated
//! string, so its spans are payload-relative; this module maps them back to the
//! original file so diagnostics point at real source.

use marqdo_expression::{Arg, Expr, RecordEntry, Statement, StatementKind};
use marqdo_markdown::{MdKind, MdNode, PayloadError};
use marqdo_span::Span;

/// One contiguous payload segment and where it came from in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    payload_start: usize,
    payload_end: usize,
    file: Span,
}

/// Map from concatenated payload offsets back to original file spans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PayloadSegments {
    segments: Vec<Segment>,
    payload_len: usize,
}

impl PayloadSegments {
    /// Build the payload string and its span map for an executable node.
    ///
    /// The strict shape rules are identical to
    /// [`marqdo_markdown::executable_payload`]: only `text` and `code` children
    /// are allowed, everything else is a hard error.
    pub fn of(node: &MdNode) -> Result<(String, PayloadSegments), PayloadError> {
        Self::from_children(&node.children)
    }

    /// Build a payload from an arbitrary child slice (used for call suffixes).
    pub fn from_children(children: &[MdNode]) -> Result<(String, PayloadSegments), PayloadError> {
        let mut payload = String::new();
        let mut segments = Vec::new();
        for child in children {
            let literal = match &child.kind {
                MdKind::Text => child.literal.as_deref().unwrap_or_default(),
                MdKind::Code { ticks } => {
                    // A code span's source span includes its backtick
                    // delimiters; the payload is only the literal inside.
                    let literal = child.literal.as_deref().unwrap_or_default();
                    let start = payload.len();
                    payload.push_str(literal);
                    let file = Span::new(
                        child.span.start.saturating_add(*ticks),
                        child.span.end.saturating_sub(*ticks),
                    );
                    segments.push(Segment {
                        payload_start: start,
                        payload_end: payload.len(),
                        file,
                    });
                    continue;
                }
                other => {
                    return Err(PayloadError {
                        span: child.span,
                        reason: format!(
                            "`{}` is not a valid payload inside an executable node",
                            other.tag()
                        ),
                    });
                }
            };
            let start = payload.len();
            payload.push_str(literal);
            segments.push(Segment {
                payload_start: start,
                payload_end: payload.len(),
                file: child.span,
            });
        }
        let payload_len = payload.len();
        Ok((
            payload,
            PayloadSegments {
                segments,
                payload_len,
            },
        ))
    }

    /// Build a single-segment payload map for one literal code span.
    pub fn single(literal: &str, file: Span) -> PayloadSegments {
        PayloadSegments {
            segments: vec![Segment {
                payload_start: 0,
                payload_end: literal.len(),
                file,
            }],
            payload_len: literal.len(),
        }
    }

    /// Payload length in bytes.
    pub fn len(&self) -> usize {
        self.payload_len
    }

    /// Whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.payload_len == 0
    }

    /// Map a payload byte offset to a file byte offset.
    ///
    /// A boundary offset belongs to the *next* segment so that a span starting
    /// exactly at a code-span boundary maps to the first byte inside the code
    /// span, not to its backtick delimiter.
    pub fn map_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.payload_len);
        for segment in &self.segments {
            if segment.payload_start <= offset && offset < segment.payload_end {
                let local = offset - segment.payload_start;
                return (segment.file.start + local).min(segment.file.end);
            }
        }
        self.segments
            .last()
            .map(|segment| segment.file.end)
            .unwrap_or(0)
    }

    /// Map a payload span to a file span.
    pub fn map_span(&self, span: Span) -> Span {
        Span::new(self.map_offset(span.start), self.map_offset(span.end))
    }

    /// Map and rewrite every span in an expression tree.
    pub fn rewrite_expr(&self, expr: &mut Expr) {
        rewrite_expr_spans(expr, self);
    }

    /// Map and rewrite every span in a parsed statement.
    pub fn rewrite_statement(&self, statement: &mut Statement) {
        statement.span = self.map_span(statement.span);
        if let StatementKind::Assign {
            name_span, value, ..
        } = &mut statement.kind
        {
            *name_span = self.map_span(*name_span);
            self.rewrite_expr(value);
        }
        if let StatementKind::Expr(expr) = &mut statement.kind {
            self.rewrite_expr(expr);
        }
    }

    /// Map and rewrite every span in a call argument list.
    pub fn rewrite_args(&self, args: &mut [Arg]) {
        for arg in args {
            arg.span = self.map_span(arg.span);
            self.rewrite_expr(&mut arg.value);
        }
    }
}

fn rewrite_expr_spans(expr: &mut Expr, segments: &PayloadSegments) {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => {}
        Expr::Ident(ident) => ident.span = segments.map_span(ident.span),
        Expr::List(items) => {
            for item in items {
                rewrite_expr_spans(item, segments);
            }
        }
        Expr::Record(entries) => {
            for entry in entries {
                entry.key_span = segments.map_span(entry.key_span);
                rewrite_expr_spans(&mut entry.value, segments);
            }
        }
        Expr::Unary { operand, span, .. } => {
            *span = segments.map_span(*span);
            rewrite_expr_spans(operand, segments);
        }
        Expr::Binary {
            left, right, span, ..
        } => {
            *span = segments.map_span(*span);
            rewrite_expr_spans(left, segments);
            rewrite_expr_spans(right, segments);
        }
        Expr::Call { callee, args, span } => {
            *span = segments.map_span(*span);
            rewrite_expr_spans(callee, segments);
            for arg in args {
                arg.span = segments.map_span(arg.span);
                rewrite_expr_spans(&mut arg.value, segments);
            }
        }
        Expr::Member { base, span, .. } => {
            *span = segments.map_span(*span);
            rewrite_expr_spans(base, segments);
        }
        Expr::Index {
            base, index, span, ..
        } => {
            *span = segments.map_span(*span);
            rewrite_expr_spans(base, segments);
            rewrite_expr_spans(index, segments);
        }
    }
}

/// Record entry is re-exported for completeness of the walker contract.
pub type _RecordEntry = RecordEntry;

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_markdown::parse;

    fn first_strong(source: &str) -> MdNode {
        let doc = parse(source);
        let mut found = None;
        doc.root.walk(&mut |node| {
            if found.is_none() && node.kind == MdKind::Strong {
                found = Some(node.clone());
            }
        });
        found.expect("strong node")
    }

    #[test]
    fn payload_and_spans_are_preserved() {
        let source = "正文 **结果 = 1 + 2** 结尾";
        let node = first_strong(source);
        let (payload, segments) = PayloadSegments::of(&node).unwrap();
        assert_eq!(payload, "结果 = 1 + 2");
        let span = segments.map_span(Span::new(0, 6));
        assert_eq!(&source[span.start..span.end], "结果");
        // "1" starts after "结果 = " (9 bytes).
        let one = segments.map_span(Span::new(9, 10));
        assert_eq!(&source[one.start..one.end], "1");
    }

    #[test]
    fn span_maps_across_text_and_code_segments() {
        let source = "**面积 = `计算(r=1)`**";
        let node = first_strong(source);
        let (payload, segments) = PayloadSegments::of(&node).unwrap();
        assert_eq!(payload, "面积 = 计算(r=1)");
        let start = payload.find("计算").unwrap();
        let call = segments.map_span(Span::new(start, start + "计算".len()));
        assert_eq!(&source[call.start..call.end], "计算");
    }

    #[test]
    fn nested_markup_is_a_hard_error() {
        let node = first_strong("**a *b* c**");
        let error = PayloadSegments::of(&node).unwrap_err();
        assert!(error.reason.contains("emph"));
    }
}
