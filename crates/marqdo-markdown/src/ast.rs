//! Owned, serializable view of the CommonMark/GFM/Math AST.
//!
//! The profile uses exactly one Markdown parser (comrak). This module converts
//! the parser's arena tree into an owned tree that keeps:
//!
//! * the node kind, so program roles can be derived from AST structure alone;
//! * the byte [`Span`] in the original file, for diagnostics and IDE support;
//! * literal payloads for inert leaves (text, code, math, HTML, front matter);
//! * the full child structure, including nodes outside the frozen profile,
//!   which are preserved as [`MdKind::Other`] rather than dropped.

use marqdo_span::Span;
use serde::{Deserialize, Serialize};

/// A node of the canonical Markdown AST view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MdNode {
    /// Structural kind and its small attribute set.
    pub kind: MdKind,
    /// Byte span in the original source (UTF-8, half open).
    pub span: Span,
    /// Literal payload for leaves that carry text (Text, Code, Math, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// Child nodes in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<MdNode>,
}

/// Structural kind of a Markdown AST node.
///
/// The variants mirror the frozen Markdown profile. Nodes produced by parser
/// extensions that are not part of the profile are preserved as `Other` so the
/// AST view never hides syntax, but they are inert for the language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MdKind {
    /// Document root.
    Document,
    /// YAML front matter block at the start of the document.
    FrontMatter,
    /// Block quote (an invocation in executable position).
    BlockQuote,
    /// Ordered or bullet list.
    List {
        /// `true` for ordered lists, `false` for bullet lists.
        ordered: bool,
        /// Ordered list start number; `None` for bullet lists.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<usize>,
        /// CommonMark tight/loose flag (a rendering distinction only).
        tight: bool,
    },
    /// One list item. Task list items (GFM) keep their checkbox state.
    Item {
        /// `Some(checked)` for a GFM task list item, `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<bool>,
    },
    /// Heading of level 1-6.
    Heading {
        /// Level 1-6.
        level: u8,
        /// Whether the source used a Setext underline.
        setext: bool,
    },
    /// Paragraph.
    Paragraph,
    /// Literal text.
    Text,
    /// Strong emphasis: an execution statement in executable position.
    Strong,
    /// Emphasis: a return statement in executable position.
    Emph,
    /// GFM strikethrough.
    Strikethrough,
    /// Inline code span.
    Code,
    /// Fenced or indented code block.
    CodeBlock {
        /// Whether the block was fenced.
        fenced: bool,
        /// Info string (language) without surrounding whitespace.
        info: String,
    },
    /// Inline math.
    Math {
        /// Whether display math (`$$...$$`).
        display: bool,
    },
    /// Block display math (`$$` fences on their own lines).
    MathBlock,
    /// Link with a destination.
    Link {
        /// Destination as written.
        url: String,
        /// Optional title.
        title: String,
    },
    /// Image with a source.
    Image {
        /// Source as written.
        url: String,
        /// Optional title.
        title: String,
    },
    /// GFM table.
    Table {
        /// Column alignments as `none`/`left`/`center`/`right`.
        alignments: Vec<String>,
    },
    /// Table row.
    TableRow {
        /// Whether this is the header row.
        header: bool,
    },
    /// Table cell.
    TableCell,
    /// Horizontal rule.
    ThematicBreak,
    /// Soft line break inside a paragraph.
    SoftBreak,
    /// Hard line break.
    LineBreak,
    /// Raw HTML block.
    HtmlBlock,
    /// Raw inline HTML.
    HtmlInline,
    /// Footnote definition (profile leaves footnotes disabled; kept for completeness).
    FootnoteDefinition {
        /// Footnote label.
        name: String,
    },
    /// Footnote reference (profile leaves footnotes disabled).
    FootnoteReference {
        /// Footnote label.
        name: String,
    },
    /// A node outside the frozen profile, preserved for inspection only.
    Other {
        /// Parser tag name.
        tag: String,
    },
}

impl MdNode {
    /// Create a leaf node.
    pub fn leaf(kind: MdKind, span: Span, literal: Option<String>) -> Self {
        Self {
            kind,
            span,
            literal,
            children: Vec::new(),
        }
    }

    /// Depth-first pre-order traversal including `self`.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a MdNode)) {
        f(self);
        for child in &self.children {
            child.walk(f);
        }
    }

    /// Direct child nodes of a given kind.
    pub fn children_of<'a>(&'a self, kind: &MdKind) -> impl Iterator<Item = &'a MdNode> {
        let kind = kind.clone();
        self.children.iter().filter(move |child| child.kind == kind)
    }

    /// Whether this node is an inline node.
    pub fn is_inline(&self) -> bool {
        matches!(
            self.kind,
            MdKind::Text
                | MdKind::Strong
                | MdKind::Emph
                | MdKind::Strikethrough
                | MdKind::Code
                | MdKind::Math { .. }
                | MdKind::Link { .. }
                | MdKind::Image { .. }
                | MdKind::SoftBreak
                | MdKind::LineBreak
                | MdKind::HtmlInline
                | MdKind::FootnoteReference { .. }
        )
    }

    /// Whether this node is a block node.
    pub fn is_block(&self) -> bool {
        !self.is_inline()
    }
}

impl MdKind {
    /// Short stable tag name used in diagnostics.
    pub const fn tag(&self) -> &'static str {
        match self {
            MdKind::Document => "document",
            MdKind::FrontMatter => "front_matter",
            MdKind::BlockQuote => "block_quote",
            MdKind::List { .. } => "list",
            MdKind::Item { .. } => "item",
            MdKind::Heading { .. } => "heading",
            MdKind::Paragraph => "paragraph",
            MdKind::Text => "text",
            MdKind::Strong => "strong",
            MdKind::Emph => "emph",
            MdKind::Strikethrough => "strikethrough",
            MdKind::Code => "code",
            MdKind::CodeBlock { .. } => "code_block",
            MdKind::Math { .. } => "math",
            MdKind::MathBlock => "math_block",
            MdKind::Link { .. } => "link",
            MdKind::Image { .. } => "image",
            MdKind::Table { .. } => "table",
            MdKind::TableRow { .. } => "table_row",
            MdKind::TableCell => "table_cell",
            MdKind::ThematicBreak => "thematic_break",
            MdKind::SoftBreak => "soft_break",
            MdKind::LineBreak => "line_break",
            MdKind::HtmlBlock => "html_block",
            MdKind::HtmlInline => "html_inline",
            MdKind::FootnoteDefinition { .. } => "footnote_definition",
            MdKind::FootnoteReference { .. } => "footnote_reference",
            MdKind::Other { .. } => "other",
        }
    }
}
