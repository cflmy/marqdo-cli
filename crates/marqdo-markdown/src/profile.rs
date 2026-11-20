//! The frozen Markdown profile: one parser, one option set, no implicit
//! upgrades.
//!
//! The parser version is pinned in `Cargo.toml` (`=1.0.0`) and the exact option
//! set is asserted by tests. Changing either is a language change and requires
//! an ADR plus regenerated goldens.

use comrak::nodes::{ListType, Node, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};
use marqdo_span::{LineIndex, Span};

use crate::ast::{MdKind, MdNode};
use crate::frontmatter::{self, FrontMatter};

/// Identifier of the frozen Markdown profile.
pub const PROFILE_ID: &str = "marqdo.md.v1";
/// Exact comrak version the profile is validated against.
pub const COMRAK_VERSION: &str = "1.0.0";

/// Build the single supported parser configuration.
///
/// Enabled: CommonMark 0.31.2 core, GFM tables, GFM strikethrough, GFM task
/// lists, `$...$` / `$$...$$` math, and YAML front matter delimited by `---`
/// at the very start of the document.
///
/// Disabled: autolinks, footnotes, description lists, wikilinks, alerts,
/// directives, superscript/subscript, raw-HTML suppression changes and every
/// other optional extension. These defaults are asserted in tests.
pub fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    options.extension.math_dollars_block = true;
    options.extension.front_matter_delimiter = Some("---".to_string());
    options
}

/// A parsed Markdown document under the frozen profile.
#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    /// Original source bytes as UTF-8 (including a BOM if present).
    pub source: String,
    /// Number of BOM bytes stripped before parsing; all spans are re-based onto
    /// the original file so they stay file offsets.
    pub bom_len: usize,
    /// Canonical AST root.
    pub root: MdNode,
    /// Parsed front matter, when present at the document start.
    pub frontmatter: Option<FrontMatter>,
    /// Line index over the original source, for diagnostics.
    pub line_index: LineIndex,
}

impl MarkdownDocument {
    /// Map a byte offset in the original source to a 1-based line/column.
    pub fn line_col(&self, offset: usize) -> marqdo_span::LineCol {
        self.line_index.line_col(offset)
    }

    /// Borrow the front matter, if the document has any.
    pub fn frontmatter(&self) -> Option<&FrontMatter> {
        self.frontmatter.as_ref()
    }
}

/// Parse a source document with the frozen profile.
///
/// Parsing never executes anything and never fails: malformed constructs that
/// the Markdown grammar itself accepts are reported later by the semantic
/// layer, which knows which AST positions are executable.
pub fn parse(source: &str) -> MarkdownDocument {
    let (bom_len, body) = strip_bom(source);
    let body_index = LineIndex::new(body);
    let arena = Arena::new();
    let root = parse_document(&arena, body, &options());
    let root = convert(&root, &body_index, bom_len, body.len());
    let frontmatter = root
        .children
        .iter()
        .find(|child| child.kind == MdKind::FrontMatter)
        .map(|node| {
            frontmatter::parse_literal(node.literal.as_deref().unwrap_or_default(), node.span)
        });
    MarkdownDocument {
        source: source.to_string(),
        bom_len,
        root,
        frontmatter,
        line_index: LineIndex::new(source),
    }
}

fn strip_bom(source: &str) -> (usize, &str) {
    match source.strip_prefix('\u{feff}') {
        Some(rest) => (source.len() - rest.len(), rest),
        None => (0, source),
    }
}

fn convert(node: &Node<'_>, index: &LineIndex, shift: usize, source_len: usize) -> MdNode {
    let data = node.data();
    let span = span_of(data.sourcepos, index, shift, source_len);
    let (kind, literal) = map_value(&data.value);
    let children = node
        .children()
        .map(|child| convert(&child, index, shift, source_len))
        .collect();
    MdNode {
        kind,
        span,
        literal,
        children,
    }
}

/// Convert a comrak source position (1-based line, 1-based byte column,
/// inclusive end) into a half-open byte span in the original file.
fn span_of(pos: Sourcepos, index: &LineIndex, shift: usize, source_len: usize) -> Span {
    // Offsets are computed in the BOM-stripped body, clamped there, and only
    // then re-based onto the original file.
    let start = offset_of(pos.start.line, pos.start.column, index, false)
        .map(|o| o.min(source_len))
        .unwrap_or(0);
    let end = offset_of(pos.end.line, pos.end.column, index, true)
        .map(|o| o.min(source_len))
        .unwrap_or(start);
    let end = end.max(start);
    Span::new(start + shift, end + shift)
}

fn offset_of(line: usize, column: usize, index: &LineIndex, inclusive: bool) -> Option<usize> {
    if line == 0 {
        return None;
    }
    let line_start = index.line_start(line)?;
    let zero_based = column.saturating_sub(1);
    Some(line_start + zero_based + usize::from(inclusive))
}

fn map_value(value: &NodeValue) -> (MdKind, Option<String>) {
    match value {
        NodeValue::Document => (MdKind::Document, None),
        NodeValue::FrontMatter(raw) => (MdKind::FrontMatter, Some(raw.clone())),
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => (MdKind::BlockQuote, None),
        NodeValue::List(list) => (
            MdKind::List {
                ordered: list.list_type == ListType::Ordered,
                start: if list.list_type == ListType::Ordered {
                    Some(list.start)
                } else {
                    None
                },
                tight: list.tight,
            },
            None,
        ),
        NodeValue::Item(_) => (MdKind::Item { task: None }, None),
        NodeValue::Heading(heading) => (
            MdKind::Heading {
                level: heading.level,
                setext: heading.setext,
            },
            None,
        ),
        NodeValue::Paragraph => (MdKind::Paragraph, None),
        NodeValue::Text(text) => (MdKind::Text, Some(text.to_string())),
        NodeValue::Strong(_) => (MdKind::Strong, None),
        NodeValue::Emph(_) => (MdKind::Emph, None),
        NodeValue::Strikethrough(_) => (MdKind::Strikethrough, None),
        NodeValue::Code(code) => (
            MdKind::Code {
                ticks: code.num_backticks,
            },
            Some(code.literal.clone()),
        ),
        NodeValue::CodeBlock(block) => (
            MdKind::CodeBlock {
                fenced: block.fenced,
                info: block.info.trim().to_string(),
            },
            Some(block.literal.clone()),
        ),
        NodeValue::Math(math) => (
            MdKind::Math {
                display: math.display_math,
            },
            Some(math.literal.clone()),
        ),
        NodeValue::MathBlock(block) => (MdKind::MathBlock, Some(block.literal.clone())),
        NodeValue::Link(link) => (
            MdKind::Link {
                url: link.url.clone(),
                title: link.title.clone(),
            },
            None,
        ),
        NodeValue::Image(image) => (
            MdKind::Image {
                url: image.url.clone(),
                title: image.title.clone(),
            },
            None,
        ),
        NodeValue::Table(table) => (
            MdKind::Table {
                alignments: table
                    .alignments
                    .iter()
                    .map(|alignment| {
                        match alignment {
                            TableAlignment::Left => "left",
                            TableAlignment::Center => "center",
                            TableAlignment::Right => "right",
                            TableAlignment::None => "none",
                        }
                        .to_string()
                    })
                    .collect(),
            },
            None,
        ),
        NodeValue::TableRow(row) => (MdKind::TableRow { header: row.header }, None),
        NodeValue::TableCell => (MdKind::TableCell, None),
        NodeValue::ThematicBreak(_) => (MdKind::ThematicBreak, None),
        NodeValue::SoftBreak => (MdKind::SoftBreak, None),
        NodeValue::LineBreak => (MdKind::LineBreak, None),
        NodeValue::HtmlBlock(block) => (MdKind::HtmlBlock, Some(block.literal.clone())),
        NodeValue::HtmlInline(raw) => (MdKind::HtmlInline, Some(raw.clone())),
        NodeValue::TaskItem(item) => (
            MdKind::Item {
                task: Some(item.symbol.is_some()),
            },
            None,
        ),
        NodeValue::FootnoteDefinition(definition) => (
            MdKind::FootnoteDefinition {
                name: definition.name.clone(),
            },
            None,
        ),
        NodeValue::FootnoteReference(reference) => (
            MdKind::FootnoteReference {
                name: reference.name.clone(),
            },
            None,
        ),
        other => (
            MdKind::Other {
                tag: tag_name(other),
            },
            literal_of(other),
        ),
    }
}

fn literal_of(value: &NodeValue) -> Option<String> {
    match value {
        NodeValue::Raw(raw) => Some(raw.clone()),
        _ => None,
    }
}

/// Turn an unmodeled parser node into a stable snake_case tag. The node is
/// inert for the language but must remain visible in `inspect --ast`.
fn tag_name(value: &NodeValue) -> String {
    let debug = format!("{value:?}");
    let head = debug
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("unknown");
    let mut out = String::new();
    for (i, ch) in head.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::MdKind;

    fn kind_tree(source: &str) -> MdNode {
        parse(source).root
    }

    fn find<'a>(node: &'a MdNode, kind: &MdKind) -> Option<&'a MdNode> {
        if &node.kind == kind {
            return Some(node);
        }
        node.children.iter().find_map(|child| find(child, kind))
    }

    #[test]
    fn profile_options_are_frozen() {
        let options = options();
        assert!(options.extension.table);
        assert!(options.extension.strikethrough);
        assert!(options.extension.tasklist);
        assert!(options.extension.math_dollars);
        assert!(options.extension.math_dollars_block);
        assert_eq!(
            options.extension.front_matter_delimiter.as_deref(),
            Some("---")
        );
        assert!(!options.extension.autolink);
        assert!(!options.extension.footnotes);
        assert!(!options.extension.description_lists);
        assert!(!options.extension.wikilinks_title_after_pipe);
        assert!(!options.extension.wikilinks_title_before_pipe);
        assert!(!options.extension.alerts);
        assert!(!options.parse.smart);
        assert!(!options.parse.ignore_setext);
        assert!(!options.extension.inline_footnotes);
    }

    #[test]
    fn parse_maps_core_constructs() {
        let root = kind_tree("# T\n\n## F\n\n**x = 1**\n\n*y*\n\n> 打印(1)\n");
        assert_eq!(root.kind, MdKind::Document);
        assert!(find(
            &root,
            &MdKind::Heading {
                level: 1,
                setext: false
            }
        )
        .is_some());
        assert!(find(&root, &MdKind::Strong).is_some());
        assert!(find(&root, &MdKind::Emph).is_some());
        assert!(find(&root, &MdKind::BlockQuote).is_some());
    }

    #[test]
    fn spans_are_byte_offsets_into_original_source() {
        let source = "# 数学\n\n正文\n";
        let doc = parse(source);
        let heading = &doc.root.children[0];
        assert_eq!(
            heading.kind,
            MdKind::Heading {
                level: 1,
                setext: false
            }
        );
        assert_eq!(&source[heading.span.start..heading.span.end], "# 数学");
        assert_eq!(doc.line_col(heading.span.start).line, 1);
    }

    #[test]
    fn bom_is_tolerated_and_spans_shift() {
        let source = "\u{feff}# T\n";
        let doc = parse(source);
        assert_eq!(doc.bom_len, 3);
        let heading = &doc.root.children[0];
        assert_eq!(heading.span.start, 3);
        assert_eq!(&source[heading.span.start..heading.span.end], "# T");
    }

    #[test]
    fn math_nodes_are_native_and_inert() {
        let doc = parse("inline $a+b$ math\n\n$$\nx+y\n$$\n");
        assert!(find(&doc.root, &MdKind::Math { display: false }).is_some());
        assert!(find(&doc.root, &MdKind::MathBlock).is_some());
    }

    #[test]
    fn setext_headings_are_heading_nodes() {
        let doc = parse("函数\n---\n");
        let heading = &doc.root.children[0];
        assert_eq!(
            heading.kind,
            MdKind::Heading {
                level: 2,
                setext: true
            }
        );
    }

    #[test]
    fn tables_and_task_lists_are_supported() {
        let doc = parse("| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n");
        assert!(find(
            &doc.root,
            &MdKind::Table {
                alignments: vec!["none".into(), "none".into()]
            }
        )
        .is_some());
        let mut checked = Vec::new();
        doc.root.walk(&mut |node| {
            if let MdKind::Item { task: Some(state) } = node.kind {
                checked.push(state);
            }
        });
        assert_eq!(checked, vec![true, false]);
    }
}
