//! Semantic mapping: canonical Markdown AST → executable IR.
//!
//! This is the only place where executable roles are assigned. Roles come from
//! node kind plus structural position (see `spec/mapping.md`); payload text is
//! parsed only after the role is known. Invalid executable shapes produce hard
//! diagnostics and never fall back to prose.

use marqdo_expression::{parse_expression, parse_statement, AssignTarget, Expr, StatementKind};
use marqdo_ir::{PlaceTarget, Stmt};
use marqdo_markdown::{normalize_name, MarkdownDocument, MdKind, MdNode};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

use crate::payload::PayloadSegments;
use crate::units::{FunctionScope, UnitIndex};

/// A mapped function body plus diagnostics.
#[derive(Debug, Clone, Default)]
pub struct MappedBody {
    /// Executable statements in document order.
    pub stmts: Vec<Stmt>,
    /// Diagnostics produced while mapping.
    pub diagnostics: Vec<Diagnostic>,
}

/// Map one function scope to executable statements.
pub fn map_function(
    doc: &MarkdownDocument,
    index: &UnitIndex,
    scope: &FunctionScope,
) -> MappedBody {
    let mut mapper = Mapper {
        diagnostics: Vec::new(),
        saw_params: None,
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
    }
}

struct Mapper {
    diagnostics: Vec<Diagnostic>,
    saw_params: Option<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    End,
    Else,
    Eof,
}

enum Payload {
    If(Expr),
    Else,
    While(Expr),
    End,
    Return(Expr),
    Params,
    Statement(marqdo_expression::Statement),
}

struct RawPayload {
    payload: Payload,
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
        let (stmts, stop) = self.sequence(&raw, &mut position, false, false);
        if stop != Stop::Eof {
            self.error(
                codes::SEM_UNMATCHED_END,
                raw.get(position.saturating_sub(1)).map(|node| node.span),
                "control frame ends outside its owning function segment",
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
        if let Some(mut payload) = self.parse_payload(&text, span, &segments) {
            if let Payload::If(expr) | Payload::While(expr) | Payload::Return(expr) = &mut payload {
                segments.rewrite_expr(expr);
            }
            out.push(RawPayload {
                payload,
                span,
                segments,
                direct,
            });
        }
    }

    fn parse_payload(
        &mut self,
        text: &str,
        span: Span,
        segments: &PayloadSegments,
    ) -> Option<Payload> {
        let map = |span: Span| segments.map_span(span);
        let text = text.trim();
        if text.starts_with("params(") {
            // Signature syntax and diagnostics are owned by `signature::parse`;
            // the mapper only recognizes the declaration payload.
            return Some(Payload::Params);
        }
        if text == "params" || text.starts_with("params ") {
            self.error(
                codes::SEM_SYNTAX,
                Some(span),
                "malformed params declaration; write `params(name: Type, ...)`",
            );
            return None;
        }
        let (word, rest) = match text.split_once(char::is_whitespace) {
            Some((word, rest)) => (word, rest.trim()),
            None => (text, ""),
        };
        // Parse control expressions with the keyword replaced by spaces so the
        // expression AST keeps payload-relative (not rest-relative) spans.
        let expression_at = |rest: &str| {
            let offset = text.len().saturating_sub(rest.len());
            parse_expression(&format!("{}{}", " ".repeat(offset), rest))
        };
        match word {
            "if" => {
                if rest.is_empty() {
                    self.error(codes::SEM_SYNTAX, Some(span), "`if` requires a condition");
                    return None;
                }
                match expression_at(rest) {
                    Ok(expr) => Some(Payload::If(expr)),
                    Err(error) => {
                        self.error(
                            codes::SEM_SYNTAX,
                            Some(map(error.span)),
                            format!("invalid if condition: {}", error.message),
                        );
                        None
                    }
                }
            }
            "else" if rest.is_empty() => Some(Payload::Else),
            "else" => {
                self.error(codes::SEM_SYNTAX, Some(span), "`else` takes no expression");
                None
            }
            "while" => {
                if rest.is_empty() {
                    self.error(
                        codes::SEM_SYNTAX,
                        Some(span),
                        "`while` requires a condition",
                    );
                    return None;
                }
                match expression_at(rest) {
                    Ok(expr) => Some(Payload::While(expr)),
                    Err(error) => {
                        self.error(
                            codes::SEM_SYNTAX,
                            Some(map(error.span)),
                            format!("invalid while condition: {}", error.message),
                        );
                        None
                    }
                }
            }
            "end" if rest.is_empty() => Some(Payload::End),
            "end" => {
                self.error(codes::SEM_SYNTAX, Some(span), "`end` takes no expression");
                None
            }
            "return" => {
                if rest.is_empty() {
                    self.error(
                        codes::SEM_SYNTAX,
                        Some(span),
                        "`return` requires an expression",
                    );
                    return None;
                }
                match expression_at(rest) {
                    Ok(expr) => Some(Payload::Return(expr)),
                    Err(error) => {
                        self.error(
                            codes::SEM_SYNTAX,
                            Some(map(error.span)),
                            format!("invalid return expression: {}", error.message),
                        );
                        None
                    }
                }
            }
            "params" => {
                if text.starts_with("params(") && text.ends_with(')') {
                    Some(Payload::Params)
                } else {
                    self.error(
                        codes::SEM_SYNTAX,
                        Some(span),
                        "malformed params declaration",
                    );
                    None
                }
            }
            "ifdef" | "returning" | "paramsx" => self.statement_payload(text, span),
            _ => self.statement_payload(text, span),
        }
    }

    fn statement_payload(&mut self, text: &str, span: Span) -> Option<Payload> {
        match parse_statement(text) {
            Ok(statement) => Some(Payload::Statement(statement)),
            Err(error) => {
                self.error_with_hint(
                    codes::SEM_SYNTAX,
                    Some(span),
                    format!("invalid execution payload: {}", error.message),
                    "one payload is exactly one statement; put control keywords in their own carriers",
                );
                None
            }
        }
    }

    fn sequence(
        &mut self,
        nodes: &[RawPayload],
        position: &mut usize,
        stop_on_else: bool,
        in_else_body: bool,
    ) -> (Vec<Stmt>, Stop) {
        let mut out = Vec::new();
        while *position < nodes.len() {
            let raw = &nodes[*position];
            match &raw.payload {
                Payload::Else => {
                    if stop_on_else {
                        return (out, Stop::Else);
                    }
                    self.error(
                        if in_else_body {
                            codes::SEM_DUPLICATE_ELSE
                        } else {
                            codes::SEM_UNMATCHED_ELSE
                        },
                        Some(raw.span),
                        if in_else_body {
                            "`if` has more than one `else`"
                        } else {
                            "`else` has no open `if`"
                        },
                    );
                    *position += 1;
                }
                Payload::End => {
                    return (out, Stop::End);
                }
                Payload::If(condition) => {
                    let condition = condition.clone();
                    let span = raw.span;
                    *position += 1;
                    let (then_body, stop) = self.sequence(nodes, position, true, false);
                    let mut else_body = Vec::new();
                    let finish = match stop {
                        Stop::Else => {
                            *position += 1;
                            let (body, stop) = self.sequence(nodes, position, false, true);
                            else_body = body;
                            stop
                        }
                        other => other,
                    };
                    if finish != Stop::End {
                        self.error(
                            codes::SEM_UNCLOSED_CONTROL,
                            Some(span),
                            "`if` is not closed by `end`",
                        );
                    } else {
                        *position += 1;
                    }
                    let mut arms = vec![marqdo_ir::BranchArm {
                        condition,
                        condition_span: span,
                        body: then_body,
                        span,
                    }];
                    if !else_body.is_empty() {
                        arms.push(marqdo_ir::BranchArm {
                            condition: Expr::Bool(true),
                            condition_span: span,
                            body: else_body,
                            span,
                        });
                    }
                    out.push(Stmt::Branch { arms, span });
                }
                Payload::While(condition) => {
                    let condition = condition.clone();
                    let span = raw.span;
                    *position += 1;
                    let (body, stop) = self.sequence(nodes, position, false, false);
                    if stop != Stop::End {
                        self.error(
                            codes::SEM_UNCLOSED_CONTROL,
                            Some(span),
                            "`while` is not closed by `end`",
                        );
                    } else {
                        *position += 1;
                    }
                    out.push(Stmt::Loop {
                        condition,
                        condition_span: span,
                        body,
                        span,
                    });
                }
                Payload::Return(value) => {
                    out.push(Stmt::Return {
                        value: value.clone(),
                        span: raw.span,
                    });
                    *position += 1;
                }
                Payload::Params => {
                    let duplicate = self.saw_params.is_some();
                    if !raw.direct {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` must be a direct payload, not inside a list or block quote",
                        );
                    } else if *position != 0 {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` must be the first payload of an Exec H2",
                        );
                    } else if duplicate {
                        self.error(
                            codes::SEM_SIGNATURE_POSITION,
                            Some(raw.span),
                            "`params(...)` may be declared only once per function",
                        );
                    } else {
                        self.saw_params = Some(raw.span);
                    }
                    *position += 1;
                }
                Payload::Statement(statement) => {
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
        (out, Stop::Eof)
    }
}

/// Normalise a heading name (re-exported for callers that resolve link targets).
pub fn normalized_heading_name(node: &MdNode) -> Option<String> {
    normalize_name(&marqdo_markdown::plain_text(node))
}
