//! Explicit projector from the canonical Markdown AST to the public document
//! shape (spec/markdown-profile.md §5, spec/analysis.md §3.1).
//!
//! The same projector is used by Analysis and `inspect --ast`; there is no
//! second tree and no serde dump of an internal Rust enum.

use marqdo_markdown::{MdKind, MdNode};
use marqdo_span::Span;
use serde_json::{json, Map, Value};

/// Project one document root. The root span is normalised to the whole source
/// (including a tolerated BOM); child spans keep their parser coordinates.
pub fn document_json(root: &MdNode, source_len: usize) -> Value {
    node_json(root, Some(Span::new(0, source_len)))
}

fn node_json(node: &MdNode, span_override: Option<Span>) -> Value {
    let span = span_override.unwrap_or(node.span);
    let mut object = Map::new();
    object.insert("kind".to_string(), json!(node.kind.tag()));
    object.insert(
        "span".to_string(),
        json!({"start": span.start, "end": span.end}),
    );
    let children: Vec<Value> = node
        .children
        .iter()
        .map(|child| node_json(child, None))
        .collect();
    object.insert("children".to_string(), Value::Array(children));
    match &node.kind {
        MdKind::Document
        | MdKind::BlockQuote
        | MdKind::Paragraph
        | MdKind::Strong
        | MdKind::Emph
        | MdKind::Strikethrough
        | MdKind::TableCell
        | MdKind::ThematicBreak
        | MdKind::SoftBreak
        | MdKind::LineBreak => {}
        MdKind::FrontMatter => {
            object.insert(
                "literal".to_string(),
                Value::String(node.literal.clone().unwrap_or_default()),
            );
        }
        MdKind::Text | MdKind::MathBlock | MdKind::HtmlBlock | MdKind::HtmlInline => {
            object.insert(
                "literal".to_string(),
                Value::String(node.literal.clone().unwrap_or_default()),
            );
        }
        MdKind::Code { ticks } => {
            object.insert("ticks".to_string(), json!(ticks));
            object.insert(
                "literal".to_string(),
                Value::String(node.literal.clone().unwrap_or_default()),
            );
        }
        MdKind::CodeBlock { fenced, info } => {
            object.insert("fenced".to_string(), json!(fenced));
            object.insert("info".to_string(), json!(info));
            object.insert(
                "literal".to_string(),
                Value::String(node.literal.clone().unwrap_or_default()),
            );
        }
        MdKind::Math { display } => {
            object.insert("display".to_string(), json!(display));
            object.insert(
                "literal".to_string(),
                Value::String(node.literal.clone().unwrap_or_default()),
            );
        }
        MdKind::Heading { level, setext } => {
            object.insert("level".to_string(), json!(level));
            object.insert("setext".to_string(), json!(setext));
        }
        MdKind::List {
            ordered,
            start,
            tight,
        } => {
            object.insert("ordered".to_string(), json!(ordered));
            object.insert("start".to_string(), json!(start));
            object.insert("tight".to_string(), json!(tight));
        }
        MdKind::Item { task } => {
            object.insert("task".to_string(), json!(task));
        }
        MdKind::Link { url, title } | MdKind::Image { url, title } => {
            object.insert("url".to_string(), json!(url));
            object.insert("title".to_string(), json!(title));
        }
        MdKind::Table { alignments } => {
            object.insert("alignments".to_string(), json!(alignments));
        }
        MdKind::TableRow { header } => {
            object.insert("header".to_string(), json!(header));
        }
        MdKind::FootnoteDefinition { .. } | MdKind::FootnoteReference { .. } => {
            // The frozen profile disables footnotes; the semantic validator
            // rejects them if a future parser change ever emits one.
            object.insert("tag".to_string(), json!(node.kind.tag()));
        }
        MdKind::Other { tag } => {
            object.insert("tag".to_string(), json!(tag));
            if let Some(literal) = &node.literal {
                object.insert("literal".to_string(), Value::String(literal.clone()));
            }
        }
    }
    Value::Object(object)
}
