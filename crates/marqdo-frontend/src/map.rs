//! Semantic mapping: canonical Markdown AST → executable IR.
//!
//! This is the only place where executable roles are assigned. Roles come from
//! node kind plus structural position (see `spec/mapping.md`); payload text is
//! parsed only after the role is known. Invalid executable shapes produce hard
//! diagnostics and never fall back to prose.

use marqdo_expression::{
    parse_arguments, parse_expression, parse_statement, Arg, Expr, StatementKind,
};
use marqdo_ir::{CallSite, CallTarget, ImportDecl, Stmt};
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
    imports: &[ImportDecl],
    scope: &FunctionScope,
) -> MappedBody {
    let mut mapper = Mapper {
        index,
        imports,
        diagnostics: Vec::new(),
    };
    let blocks = &doc.root.children[scope.body_start..scope.body_end];
    let stmts = mapper.blocks(blocks);
    MappedBody {
        stmts,
        diagnostics: mapper.diagnostics,
    }
}

struct Mapper<'a> {
    index: &'a UnitIndex,
    #[allow(dead_code)]
    imports: &'a [ImportDecl],
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Mapper<'a> {
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
        let mut stmts = Vec::new();
        for block in blocks {
            stmts.extend(self.block(block));
        }
        stmts
    }

    fn block(&mut self, block: &MdNode) -> Vec<Stmt> {
        match &block.kind {
            MdKind::Paragraph => self.paragraph(block),
            MdKind::List { ordered, .. } => self.list(block, *ordered),
            MdKind::BlockQuote => self.quote(block),
            // Level>=3 headings and inert document values stay out of the IR.
            _ => Vec::new(),
        }
    }

    fn paragraph(&mut self, paragraph: &MdNode) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        for child in &paragraph.children {
            match &child.kind {
                MdKind::Strong => {
                    if let Some(stmt) = self.exec_statement(child) {
                        stmts.push(stmt);
                    }
                }
                MdKind::Emph => {
                    if let Some(stmt) = self.return_statement(child) {
                        stmts.push(stmt);
                    }
                }
                // Lazy/inert inline structures: never recurse into them.
                MdKind::Link { .. }
                | MdKind::Image { .. }
                | MdKind::Strikethrough
                | MdKind::Code { .. }
                | MdKind::Math { .. }
                | MdKind::Text
                | MdKind::SoftBreak
                | MdKind::LineBreak
                | MdKind::HtmlInline
                | MdKind::FootnoteReference { .. } => {}
                _ => {}
            }
        }
        stmts
    }

    fn exec_statement(&mut self, node: &MdNode) -> Option<Stmt> {
        let (payload, segments) = match PayloadSegments::of(node) {
            Ok(result) => result,
            Err(error) => {
                self.error_with_hint(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(error.span),
                    error.reason,
                    "keep the executable payload as plain text or a single code span",
                );
                return None;
            }
        };
        if payload.trim().is_empty() {
            self.error(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(node.span),
                "execution statement has an empty payload",
            );
            return None;
        }
        match parse_statement(&payload) {
            Ok(mut statement) => {
                segments.rewrite_statement(&mut statement);
                Some(match statement.kind {
                    StatementKind::Assign {
                        name,
                        name_span,
                        value,
                    } => Stmt::Assign {
                        name,
                        name_span,
                        value,
                        span: node.span,
                    },
                    StatementKind::Expr(expr) => Stmt::Eval {
                        expr,
                        span: node.span,
                    },
                })
            }
            Err(error) => {
                self.error_with_hint(
                    codes::EXPR_PARSE,
                    Some(segments.map_span(error.span)),
                    format!("invalid execution statement: {}", error.message),
                    "statements are `name = expression` or an expression; put the payload in a code span when Markdown emphasis conflicts",
                );
                None
            }
        }
    }

    fn return_statement(&mut self, node: &MdNode) -> Option<Stmt> {
        let (payload, segments) = match PayloadSegments::of(node) {
            Ok(result) => result,
            Err(error) => {
                self.error_with_hint(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(error.span),
                    error.reason,
                    "a return payload must be plain text or a single code span",
                );
                return None;
            }
        };
        if payload.trim().is_empty() {
            self.error(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(node.span),
                "return statement has an empty payload",
            );
            return None;
        }
        match parse_expression(&payload) {
            Ok(mut expr) => {
                segments.rewrite_expr(&mut expr);
                Some(Stmt::Return {
                    value: expr,
                    span: node.span,
                })
            }
            Err(error) => {
                self.error_with_hint(
                    codes::EXPR_PARSE,
                    Some(segments.map_span(error.span)),
                    format!("invalid return expression: {}", error.message),
                    "`*表达式*` returns the value of the expression",
                );
                None
            }
        }
    }

    fn list(&mut self, list: &MdNode, ordered: bool) -> Vec<Stmt> {
        if list.children.is_empty() {
            self.error(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(list.span),
                "list in an executable position has no items",
            );
            return Vec::new();
        }
        let mut arms = Vec::new();
        let mut loops = Vec::new();
        for item in &list.children {
            let Some((condition, condition_span, body_blocks)) = self.condition_of(item) else {
                continue;
            };
            let body = self.blocks(body_blocks);
            if ordered {
                arms.push(marqdo_ir::BranchArm {
                    condition,
                    condition_span,
                    body,
                    span: item.span,
                });
            } else {
                loops.push(Stmt::Loop {
                    condition,
                    condition_span,
                    body,
                    span: item.span,
                });
            }
        }
        if ordered {
            if arms.is_empty() {
                return Vec::new();
            }
            vec![Stmt::Branch {
                arms,
                span: list.span,
            }]
        } else {
            loops
        }
    }

    /// Extract the condition header of a list item and its remaining blocks.
    fn condition_of<'b>(&mut self, item: &'b MdNode) -> Option<(Expr, Span, &'b [MdNode])> {
        let Some(first) = item.children.first() else {
            self.error_with_hint(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(item.span),
                "list item in an executable position must start with a condition code span",
                "write the condition first: `` `n > 0` ``",
            );
            return None;
        };
        if !matches!(first.kind, MdKind::Paragraph) {
            self.error_with_hint(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(first.span),
                "list item must start with a paragraph containing the condition code span",
                "put the condition on the first line of the item",
            );
            return None;
        }
        let Some(condition_node) = first.children.first() else {
            self.error(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(first.span),
                "list item paragraph is empty; a condition code span is required",
            );
            return None;
        };
        if !matches!(condition_node.kind, MdKind::Code { .. }) {
            self.error_with_hint(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(condition_node.span),
                "list item must start with a condition inside an inline code span",
                "the condition is a `Code` span: `` `n > 0` ``; other text is description",
            );
            return None;
        }
        // The remainder of the condition paragraph is description only. The
        // semantic mapper does not descend into it, so an executable `Strong`
        // or `Emph` there would otherwise be silently swallowed. Reject it as
        // a hard shape error instead of guessing the author's intent.
        for tail in first.children.iter().skip(1) {
            if matches!(tail.kind, MdKind::Strong | MdKind::Emph) {
                self.error_with_hint(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(tail.span),
                    format!(
                        "`{}` inside a list-item condition paragraph is not executed and cannot be silently ignored",
                        tail.kind.tag()
                    ),
                    "put executable statements in their own paragraph after the condition line",
                );
                return None;
            }
        }
        let literal = condition_node.literal.clone().unwrap_or_default();
        // `condition_node.span` includes the backtick delimiters. The payload
        // map must start at the first literal byte so parse diagnostics and
        // expression spans point at the expression, not at a delimiter.
        let ticks = match condition_node.kind {
            MdKind::Code { ticks } => ticks,
            _ => 0,
        };
        let literal_span = Span::new(
            condition_node.span.start.saturating_add(ticks),
            condition_node.span.end.saturating_sub(ticks),
        );
        let segments = PayloadSegments::single(&literal, literal_span);
        match parse_expression(&literal) {
            Ok(mut expr) => {
                segments.rewrite_expr(&mut expr);
                Some((expr, condition_node.span, &item.children[1..]))
            }
            Err(error) => {
                self.error_with_hint(
                    codes::EXPR_PARSE,
                    Some(segments.map_span(error.span)),
                    format!("invalid condition expression: {}", error.message),
                    "conditions are Boolean expressions, e.g. `n > 0`",
                );
                None
            }
        }
    }

    fn quote(&mut self, quote: &MdNode) -> Vec<Stmt> {
        if quote.children.len() != 1 || quote.children[0].kind != MdKind::Paragraph {
            self.error_with_hint(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(quote.span),
                "a block quote invocation must contain exactly one paragraph",
                "use one block quote per call; prose belongs in normal paragraphs",
            );
            return Vec::new();
        }
        let paragraph = &quote.children[0];
        self.call_paragraph(quote, paragraph)
    }

    fn call_paragraph(&mut self, quote: &MdNode, paragraph: &MdNode) -> Vec<Stmt> {
        let first = paragraph.children.first();
        let is_link_form = first
            .map(|node| matches!(node.kind, MdKind::Link { .. }))
            .unwrap_or(false);
        if is_link_form {
            return self.link_call(quote, paragraph);
        }
        let (payload, segments) = match PayloadSegments::of(paragraph) {
            Ok(result) => result,
            Err(error) => {
                self.error_with_hint(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(error.span),
                    error.reason,
                    "a block quote call payload must be plain text or one code span",
                );
                return Vec::new();
            }
        };
        if payload.trim().is_empty() {
            self.error(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(quote.span),
                "block quote invocation has an empty payload",
            );
            return Vec::new();
        }
        match parse_expression(&payload) {
            Ok(mut expr) => {
                segments.rewrite_expr(&mut expr);
                if !matches!(expr, Expr::Call { .. }) {
                    self.error_with_hint(
                        codes::MARKDOWN_INVALID_CONSTRUCT,
                        Some(segments.map_span(expr_span(&expr))),
                        "a block quote must be a complete function call",
                        "write `> 函数(参数)`; use a `**strong**` statement to bind a result",
                    );
                    return Vec::new();
                }
                vec![Stmt::Eval {
                    expr,
                    span: quote.span,
                }]
            }
            Err(error) => {
                self.error_with_hint(
                    codes::EXPR_PARSE,
                    Some(segments.map_span(error.span)),
                    format!("invalid call expression: {}", error.message),
                    "block quotes call functions explicitly: `> 函数(参数)`",
                );
                Vec::new()
            }
        }
    }

    fn link_call(&mut self, quote: &MdNode, paragraph: &MdNode) -> Vec<Stmt> {
        let link = &paragraph.children[0];
        let (url, link_span) = match &link.kind {
            MdKind::Link { url, .. } => (url.clone(), link.span),
            _ => unreachable!("link form checked by caller"),
        };
        let target = self.resolve_link_target(&url, link_span);
        let rest = &paragraph.children[1..];
        let (suffix, segments) = match PayloadSegments::from_children(rest) {
            Ok(result) => result,
            Err(error) => {
                self.error_with_hint(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(error.span),
                    error.reason,
                    "link call arguments must follow the link as plain text or code spans",
                );
                return Vec::new();
            }
        };
        if suffix.trim().is_empty() {
            self.error_with_hint(
                codes::MARKDOWN_INVALID_CONSTRUCT,
                Some(quote.span),
                "link-form call requires an explicit argument list",
                "write `> [函数](#函数)()` even for zero arguments",
            );
            return Vec::new();
        }
        let mut args: Vec<Arg> = match parse_arguments(&suffix) {
            Ok(args) => args,
            Err(error) => {
                self.error_with_hint(
                    codes::EXPR_PARSE,
                    Some(segments.map_span(error.span)),
                    format!("invalid call arguments: {}", error.message),
                    "arguments are `name = value` or positional values",
                );
                return Vec::new();
            }
        };
        segments.rewrite_args(&mut args);
        let Some(target) = target else {
            return Vec::new();
        };
        vec![Stmt::Call {
            call: CallSite {
                target,
                args,
                span: quote.span,
            },
            span: quote.span,
        }]
    }

    fn resolve_link_target(&mut self, url: &str, span: Span) -> Option<CallTarget> {
        if let Some(fragment) = url.strip_prefix('#') {
            let unit_id = self.index.anchors.get(fragment).cloned().or_else(|| {
                self.index
                    .units
                    .iter()
                    .find(|unit| unit.name == fragment)
                    .map(|unit| unit.id.clone())
            });
            let Some(unit_id) = unit_id else {
                self.error(
                    codes::REF_UNRESOLVED,
                    Some(span),
                    format!("no heading matches call target `{url}`"),
                );
                return None;
            };
            let Some(scope) = self
                .index
                .functions
                .iter()
                .find(|scope| scope.unit == unit_id)
            else {
                self.error(
                    codes::REF_UNRESOLVED,
                    Some(span),
                    format!("call target `{url}` is not a function"),
                );
                return None;
            };
            Some(CallTarget::Resolved {
                module: unit_id.module.clone(),
                function: scope.id.name.clone(),
            })
        } else if url.starts_with("http://") || url.starts_with("https://") {
            self.error_with_hint(
                codes::REF_UNRESOLVED,
                Some(span),
                "external links cannot be called",
                "link targets used as calls must resolve to a Marqdo function",
            );
            None
        } else {
            self.error_with_hint(
                codes::MODULE_UNRESOLVED,
                Some(span),
                format!("cannot resolve cross-module call target `{url}`"),
                "import the module explicitly in front matter and call it through the alias",
            );
            None
        }
    }
}

fn expr_span(expr: &Expr) -> Span {
    marqdo_expression::Expr::span(expr)
}

/// Normalise a heading name (re-exported for callers that resolve link targets).
pub fn normalized_heading_name(node: &MdNode) -> Option<String> {
    normalize_name(&marqdo_markdown::plain_text(node))
}
