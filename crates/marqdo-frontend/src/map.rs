//! Semantic mapping: canonical Markdown AST → executable IR.
//!
//! This is the only place where executable roles are assigned. Roles come from
//! node kind plus structural position (see `spec/mapping.md`); payload text is
//! parsed only after the role is known. Invalid executable shapes produce hard
//! diagnostics and never fall back to prose.

use marqdo_expression::{
    parse_payload_statement, AssignTarget, Expr, PayloadStatement as Payload, StatementKind,
};
use marqdo_ir::{BranchArm, PlaceTarget, Stmt};
use marqdo_markdown::{MarkdownDocument, MdKind, MdNode};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

use crate::payload::PayloadSegments;
use crate::units::{FunctionScope, UnitIndex};

/// The `params(...)` payload accepted for one function.
///
/// Only the first function-wide direct payload qualifies; the signature parser
/// validates the interior. Keeping the carrier text and span map here means the
/// declaration is recognized by the same traversal and mode rules that decide
/// what executes, so a Knowledge section can never declare a signature.
#[derive(Debug, Clone)]
pub struct ParamsDeclaration {
    /// Payload text, e.g. `params(n: Int)`.
    pub text: String,
    /// Span of the carrier node.
    pub span: Span,
    /// Payload-offset → file-span map for default expressions.
    pub segments: PayloadSegments,
}

/// A mapped function body plus diagnostics.
#[derive(Debug, Clone, Default)]
pub struct MappedBody {
    /// Executable statements in document order.
    pub stmts: Vec<Stmt>,
    /// Diagnostics produced while mapping.
    pub diagnostics: Vec<Diagnostic>,
    /// The accepted `params(...)` declaration, if the function has one.
    pub params: Option<ParamsDeclaration>,
}

/// Map one function scope to executable statements.
pub fn map_function(
    doc: &MarkdownDocument,
    index: &UnitIndex,
    scope: &FunctionScope,
) -> MappedBody {
    let mut mapper = Mapper {
        diagnostics: Vec::new(),
        params: None,
        saw_params: None,
        executed: false,
    };
    let mut stmts = Vec::new();
    let mut segment = Vec::new();
    let mut mode = crate::units::SemanticMode::Exec;
    for (node_index, block) in doc.root.children.iter().enumerate() {
        if node_index < scope.body_start || node_index >= scope.body_end {
            continue;
        }
        if let MdKind::Heading { .. } = block.kind {
            if mode == crate::units::SemanticMode::Exec {
                stmts.extend(mapper.blocks(&segment));
            }
            segment.clear();
            if let Some(heading) = index.headings.iter().find(|h| h.node_index == node_index) {
                mode = heading.mode;
            }
            continue;
        }
        if mode == crate::units::SemanticMode::Exec {
            segment.push(block.clone());
        }
    }
    if mode == crate::units::SemanticMode::Exec {
        stmts.extend(mapper.blocks(&segment));
    }
    MappedBody {
        stmts,
        diagnostics: mapper.diagnostics,
        params: mapper.params,
    }
}

struct Mapper {
    diagnostics: Vec<Diagnostic>,
    /// Accepted `params(...)` payload for the function being mapped.
    params: Option<ParamsDeclaration>,
    /// First direct `params(...)` payload of the **function** (not just the
    /// current heading segment), used to reject duplicate declarations.
    saw_params: Option<Span>,
    /// Whether any executable statement has already been lowered for this
    /// function. `params(...)` is legal only before that point, even when the
    /// earlier statements live in an earlier heading segment.
    executed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArmContext {
    Outside,
    Conditional,
    Default,
}

struct RawPayload {
    payload: Payload,
    text: String,
    span: Span,
    segments: PayloadSegments,
    /// True only for a carrier in the heading's direct body, not inside a
    /// list or block quote (`params` is legal only there).
    direct: bool,
}

impl Mapper {
    fn error(&mut self, code: &str, span: Option<Span>, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    fn error_with_hint(
        &mut self,
        code: &str,
        span: Option<Span>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message).with_hint(hint));
    }

    fn blocks(&mut self, blocks: &[MdNode]) -> Vec<Stmt> {
        let mut raw = Vec::new();
        for block in blocks {
            self.extract(block, &mut raw, true);
        }
        let mut position = 0;
        let stmts = self.sequence(&raw, &mut position, ArmContext::Outside, false);
        if let Some(node) = raw.get(position) {
            // The only delimiter that can stop the segment is a stray `end`.
            self.error(
                codes::SEM_UNMATCHED_END,
                Some(node.span),
                "`end` has no open `if`/`while` in this heading segment",
            );
        }
        stmts
    }

    fn extract(&mut self, node: &MdNode, out: &mut Vec<RawPayload>, direct: bool) {
        match &node.kind {
            MdKind::Paragraph => {
                for child in &node.children {
                    self.extract(child, out, direct);
                }
            }
            MdKind::BlockQuote | MdKind::Item { .. } | MdKind::List { .. } => {
                for child in &node.children {
                    self.extract(child, out, false);
                }
            }
            MdKind::Strong | MdKind::Emph => {
                let (text, segments) = match PayloadSegments::of(node) {
                    Ok(result) => result,
                    Err(error) => {
                        self.error(
                            codes::SEM_PAYLOAD_NESTED,
                            Some(error.span),
                            format!("invalid payload carrier: {}", error.reason),
                        );
                        return;
                    }
                };
                self.push(text, node.span, segments, out, direct);
            }
            MdKind::Code { ticks } => {
                let literal = node.literal.as_deref().unwrap_or_default();
                let inner = Span::new(
                    node.span.start.saturating_add(*ticks),
                    node.span.end.saturating_sub(*ticks),
                );
                self.push(
                    literal.to_string(),
                    node.span,
                    PayloadSegments::single(literal, inner),
                    out,
                    direct,
                );
            }
            MdKind::Heading { .. } => {}
            _ => {}
        }
    }

    fn push(
        &mut self,
        text: String,
        span: Span,
        segments: PayloadSegments,
        out: &mut Vec<RawPayload>,
        direct: bool,
    ) {
        if text.trim().is_empty() {
            self.error(
                codes::SEM_EMPTY_PAYLOAD,
                Some(span),
                "executable payload is empty",
            );
            return;
        }
        if let Some(mut payload) = self.parse_payload(&text, &segments) {
            if let Payload::If(expr)
            | Payload::ElseIf(expr)
            | Payload::While(expr)
            | Payload::Return(expr) = &mut payload
            {
                segments.rewrite_expr(expr);
            }
            out.push(RawPayload {
                payload,
                text,
                span,
                segments,
                direct,
            });
        }
    }

    fn parse_payload(&mut self, text: &str, segments: &PayloadSegments) -> Option<Payload> {
        match parse_payload_statement(text) {
            Ok(payload) => Some(payload),
            Err(error) => {
                self.error_with_hint(
                    codes::SEM_SYNTAX,
                    Some(segments.map_span(error.span)),
                    format!("invalid execution payload: {}", error.message),
                    "one carrier is exactly one statement; control keywords need their own carrier",
                );
                None
            }
        }
    }

    /// Read a body up to its next delimiter without consuming that delimiter.
    fn sequence(
        &mut self,
        nodes: &[RawPayload],
        position: &mut usize,
        arm: ArmContext,
        in_loop: bool,
    ) -> Vec<Stmt> {
        let mut out = Vec::new();
        while let Some(raw) = nodes.get(*position) {
            match &raw.payload {
                Payload::Else | Payload::ElseIf(_) => {
                    if arm == ArmContext::Conditional {
                        break;
                    }
                    let (code, message) = match (&raw.payload, arm) {
                        (Payload::Else, ArmContext::Default) => {
                            (codes::SEM_DUPLICATE_ELSE, "`if` has more than one `else`")
                        }
                        (Payload::Else, _) => {
                            (codes::SEM_UNMATCHED_ELSE, "`else` has no open `if`")
                        }
                        (_, ArmContext::Default) => (
                            codes::SEM_ELSE_IF_AFTER_ELSE,
                            "`else if` cannot follow the final `else`",
                        ),
                        (_, _) => (codes::SEM_UNMATCHED_ELSE_IF, "`else if` has no open `if`"),
                    };
                    self.error(code, Some(raw.span), message);
                    *position += 1;
                }
                Payload::End => break,
                Payload::If(_) => {
                    self.executed = true;
                    out.push(self.branch(nodes, position, in_loop));
                }
                Payload::While(condition) => {
                    self.executed = true;
                    *position += 1;
                    let body = self.sequence(nodes, position, ArmContext::Outside, true);
                    self.close_block(nodes, position, raw.span, "while");
                    out.push(Stmt::Loop {
                        condition: condition.clone(),
                        condition_span: raw.span,
                        body,
                        span: raw.span,
                    });
                }
                Payload::Break | Payload::Continue => {
                    self.executed = true;
                    let span = raw.span;
                    let (statement, code, keyword) = match raw.payload {
                        Payload::Break => {
                            (Stmt::Break { span }, codes::SEM_BREAK_OUTSIDE_LOOP, "break")
                        }
                        _ => (
                            Stmt::Continue { span },
                            codes::SEM_CONTINUE_OUTSIDE_LOOP,
                            "continue",
                        ),
                    };
                    if in_loop {
                        out.push(statement);
                    } else {
                        self.error(
                            code,
                            Some(span),
                            format!("`{keyword}` requires an open loop in this heading segment"),
                        );
                    }
                    *position += 1;
                }
                Payload::Return(value) => {
                    self.executed = true;
                    out.push(Stmt::Return {
                        value: value.clone(),
                        span: raw.span,
                    });
                    *position += 1;
                }
                Payload::Params => {
                    if !raw.direct {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` must be a direct payload, not inside a list or block quote",
                        );
                    } else if self.saw_params.is_some() {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` may be declared only once per function",
                        );
                    } else if self.executed || *position != 0 {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` must be the first executable payload of the Exec H2,                              before any statement in any heading segment",
                        );
                    } else {
                        self.saw_params = Some(raw.span);
                        self.params = Some(ParamsDeclaration {
                            text: raw.text.clone(),
                            span: raw.span,
                            segments: raw.segments.clone(),
                        });
                    }
                    *position += 1;
                }
                Payload::Ordinary(statement) => {
                    self.executed = true;
                    let mut statement = statement.clone();
                    raw.segments.rewrite_statement(&mut statement);
                    let span = raw.span;
                    match statement.kind {
                        StatementKind::Assign { target, value } => out.push(Stmt::Assign {
                            target: match target {
                                AssignTarget::Local { name, name_span } => {
                                    PlaceTarget::Local { name, name_span }
                                }
                                AssignTarget::Index {
                                    base,
                                    index,
                                    span: target_span,
                                } => PlaceTarget::Index {
                                    base: *base,
                                    index: *index,
                                    span: target_span,
                                },
                            },
                            value,
                            span,
                        }),
                        StatementKind::Expr(expr) => out.push(Stmt::Eval { expr, span }),
                    }
                    *position += 1;
                }
            }
        }
        out
    }

    fn branch(&mut self, nodes: &[RawPayload], position: &mut usize, in_loop: bool) -> Stmt {
        let span = nodes[*position].span;
        let mut arms = Vec::new();
        while let Some(raw) = nodes.get(*position) {
            let condition = match &raw.payload {
                Payload::If(condition) | Payload::ElseIf(condition) => condition,
                _ => break,
            };
            *position += 1;
            let body = self.sequence(nodes, position, ArmContext::Conditional, in_loop);
            arms.push(BranchArm {
                condition: condition.clone(),
                condition_span: raw.span,
                body,
                span: raw.span,
            });
        }
        if matches!(
            nodes.get(*position).map(|raw| &raw.payload),
            Some(Payload::Else)
        ) {
            *position += 1;
            let body = self.sequence(nodes, position, ArmContext::Default, in_loop);
            // Preserve the v2 projection and observation of the fallback arm.
            // An empty fallback has the same effect as ordinary fallthrough.
            if !body.is_empty() {
                arms.push(BranchArm {
                    condition: Expr::Bool(true),
                    condition_span: span,
                    body,
                    span,
                });
            }
        }
        self.close_block(nodes, position, span, "if");
        Stmt::Branch { arms, span }
    }

    fn close_block(
        &mut self,
        nodes: &[RawPayload],
        position: &mut usize,
        opener: Span,
        keyword: &str,
    ) {
        if matches!(
            nodes.get(*position).map(|raw| &raw.payload),
            Some(Payload::End)
        ) {
            *position += 1;
        } else {
            self.error(
                codes::SEM_UNCLOSED_CONTROL,
                Some(opener),
                format!("`{keyword}` is not closed by `end`"),
            );
        }
    }
}
