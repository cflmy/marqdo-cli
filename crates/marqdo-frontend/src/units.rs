//! Heading scope index.
//!
//! Markdown headings are sibling blocks, so function scopes are derived from
//! heading levels, not from container nesting:
//!
//! * `heading(level=1)` -> namespace / object domain (blueprint 1.2, 2.1)
//! * `heading(level=2)` -> function
//! * `heading(level>=3)` -> named section inside the enclosing unit, never a
//!   new callable function
//!
//! The range of a heading extends to the next heading of the same or a lower
//! level (same or smaller number). Headings inside block quotes, list items or
//! code fences cannot appear as direct children of the document, hence they can
//! never create functions.

use std::collections::BTreeMap;

use marqdo_ir::{FunctionId, ModuleId, UnitId, UnitInfo, UnitKind};
use marqdo_markdown::{normalize_name, plain_text, MarkdownDocument, MdKind, MdNode};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

/// A heading that participates in the unit index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    /// Index of the heading in `Document.children`.
    pub node_index: usize,
    /// Level 1-6.
    pub level: u8,
    /// Canonical (NFC, whitespace collapsed) heading name.
    pub name: String,
    /// Heading span.
    pub span: Span,
    /// Canonical heading path including this heading.
    pub path: Vec<String>,
    /// Role derived from level and position.
    pub kind: UnitKind,
    /// Semantic mode declared or inherited by this heading.
    pub mode: SemanticMode,
}

/// A function range mapped from a level-2 heading.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionScope {
    /// Canonical function identity (namespace-qualified when namespaced).
    pub id: FunctionId,
    /// Function name as written inside its namespace.
    pub local_name: String,
    /// Enclosing level-1 namespace, if any.
    pub namespace: Option<String>,
    /// Addressable unit defined by this heading.
    pub unit: UnitId,
    /// Heading span.
    pub heading_span: Span,
    /// Range of `Document.children` indices forming the function body
    /// (`body_start..body_end`), excluding the heading itself.
    pub body_start: usize,
    /// Exclusive end of the body range.
    pub body_end: usize,
    /// Span from the heading through the last body block.
    pub span: Span,
}

/// Result of indexing a document's headings.
#[derive(Debug, Clone, Default)]
pub struct UnitIndex {
    /// Addressable units in document order (root first).
    pub units: Vec<UnitInfo>,
    /// Headings in document order.
    pub headings: Vec<HeadingInfo>,
    /// Function ranges in document order.
    pub functions: Vec<FunctionScope>,
    /// GitHub-style heading anchors mapped to units. Anchors are derived from
    /// the rendered heading text and are kept separate from canonical names and
    /// internal unit ids.
    pub anchors: BTreeMap<String, UnitId>,
    /// Diagnostics produced while building the index.
    pub diagnostics: Vec<Diagnostic>,
}

impl UnitIndex {
    /// Units declared directly under the given parent path.
    pub fn children_of<'a>(&'a self, parent: &[String]) -> impl Iterator<Item = &'a UnitInfo> + 'a {
        let parent = parent.to_vec();
        self.units.iter().filter(move |unit| {
            unit.id.path.len() == parent.len() + 1 && unit.id.path.starts_with(&parent)
        })
    }

    /// Function scope whose body range contains the given document child index.
    pub fn function_containing_block(&self, block_index: usize) -> Option<&FunctionScope> {
        self.functions
            .iter()
            .find(|f| block_index >= f.body_start && block_index < f.body_end)
    }
}

fn has_payload(node: &MdNode) -> bool {
    match &node.kind {
        MdKind::Strong | MdKind::Emph | MdKind::Code { .. } => true,
        MdKind::Paragraph | MdKind::List { .. } | MdKind::Item { .. } | MdKind::BlockQuote => {
            node.children.iter().any(has_payload)
        }
        _ => false,
    }
}

/// Build the heading/function index for a parsed document.
/// Semantic mode declared or inherited by a heading (RFC-LANG-0002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMode {
    /// Document/knowledge region; no executable statements.
    Knowledge,
    /// Executable region; only an Exec H2 is callable.
    Exec,
}

fn heading_declaration(
    node: &MdNode,
) -> (Option<SemanticMode>, Option<String>, Option<Diagnostic>) {
    let Some(first) = node.children.first() else {
        return (None, None, None);
    };
    let MdKind::Text = first.kind else {
        return (None, None, None);
    };
    let literal = first.literal.as_deref().unwrap_or_default();
    let (mode, rest) = if let Some(rest) = literal.strip_prefix("@exec") {
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return (None, None, None);
        }
        (SemanticMode::Exec, rest)
    } else if let Some(rest) = literal.strip_prefix("@knowledge") {
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return (None, None, None);
        }
        (SemanticMode::Knowledge, rest)
    } else if literal.starts_with('@')
        && literal
            .trim_start_matches('@')
            .chars()
            .next()
            .is_some_and(char::is_alphabetic)
    {
        return (
            None,
            None,
            Some(Diagnostic::error(
                codes::SEM_INVALID_MODE_MARKER,
                Some(first.span),
                "unknown heading mode marker; use exactly `@exec` or `@knowledge`",
            )),
        );
    } else {
        return (None, None, None);
    };
    let title = rest.trim();
    if title.is_empty() {
        return (
            Some(mode),
            None,
            Some(Diagnostic::error(
                codes::SEM_EMPTY_UNIT_NAME,
                Some(node.span),
                "mode declaration has an empty heading name",
            )),
        );
    }
    if title.starts_with("@exec") || title.starts_with("@knowledge") {
        return (
            Some(mode),
            None,
            Some(Diagnostic::error(
                codes::SEM_INVALID_MODE_MARKER,
                Some(node.span),
                "a heading may declare only one semantic mode",
            )),
        );
    }
    (Some(mode), Some(title.to_string()), None)
}

/// Build the heading/function index for a parsed document.
pub fn build_units(doc: &MarkdownDocument, module: &ModuleId) -> UnitIndex {
    let mut index = UnitIndex::default();
    index.units.push(UnitInfo {
        id: UnitId {
            module: module.clone(),
            path: Vec::new(),
        },
        level: 0,
        name: String::new(),
        span: doc.root.span,
        kind: UnitKind::Root,
        function: None,
    });

    // Stack of (level, path, mode) for ancestors.
    let mut stack: Vec<(u8, Vec<String>, SemanticMode)> = Vec::new();
    let mut seen: BTreeMap<(Vec<String>, String), Span> = BTreeMap::new();
    let mut current_namespace: Option<String> = None;
    let mut current_function: Option<FunctionId> = None;
    let mut heading_infos: Vec<HeadingInfo> = Vec::new();

    for (node_index, child) in doc.root.children.iter().enumerate() {
        let MdKind::Heading { level, .. } = &child.kind else {
            continue;
        };
        let level = *level;
        let (declared, declared_name, declaration_error) = heading_declaration(child);
        if let Some(error) = declaration_error {
            index.diagnostics.push(error);
            if declared_name.is_none() {
                continue;
            }
        }
        let inherited = stack
            .last()
            .map(|(_, _, mode)| *mode)
            .unwrap_or(SemanticMode::Knowledge);
        let mode = declared.unwrap_or(inherited);
        let raw_name = match declared_name {
            Some(name) => name,
            None => plain_text(child),
        };
        let Some(name) = normalize_name(&raw_name) else {
            index.diagnostics.push(
                Diagnostic::error(
                    codes::SEM_EMPTY_UNIT_NAME,
                    Some(child.span),
                    "heading name must not be empty",
                )
                .with_hint("give the heading a non-empty canonical name"),
            );
            continue;
        };

        while let Some((parent_level, _, _)) = stack.last() {
            if *parent_level >= level {
                stack.pop();
            } else {
                break;
            }
        }
        let parent_path = stack
            .last()
            .map(|(_, path, _)| path.clone())
            .unwrap_or_default();
        let path = {
            let mut path = parent_path.clone();
            path.push(name.clone());
            path
        };

        let key = (parent_path.clone(), name.clone());
        if let Some(previous) = seen.get(&key) {
            let previous = *previous;
            index.diagnostics.push(
                Diagnostic::error(
                    codes::SEM_DUPLICATE_UNIT_NAME,
                    Some(child.span),
                    format!("duplicate heading name `{name}` in the same scope"),
                )
                .with_related(Some(previous), "first declared here")
                .with_hint("rename one of the headings; duplicate names are not auto-suffixed"),
            );
        } else {
            seen.insert(key, child.span);
        }

        let kind = match level {
            1 => UnitKind::Namespace,
            2 if mode == SemanticMode::Exec => UnitKind::Function,
            2 => UnitKind::Knowledge,
            _ => UnitKind::Section,
        };
        if level == 1 {
            current_namespace = Some(name.clone());
            current_function = None;
        }
        if level >= 3 && mode == SemanticMode::Exec && current_function.is_none() {
            index.diagnostics.push(
                Diagnostic::error(
                    codes::SEM_EXEC_WITHOUT_FUNCTION,
                    Some(child.span),
                    "executable section has no owning Exec H2 function",
                )
                .with_hint("put the section under an Exec H2, or mark it @knowledge"),
            );
        }
        let function = if level == 2 && mode == SemanticMode::Exec {
            let local = name.clone();
            let qualified = match &current_namespace {
                Some(namespace) => format!("{namespace}.{local}"),
                None => local.clone(),
            };
            let id = FunctionId {
                module: module.clone(),
                name: qualified,
            };
            current_function = Some(id.clone());
            Some(id)
        } else if level >= 3 && mode == SemanticMode::Exec {
            current_function.clone()
        } else {
            if level <= 2 {
                current_function = None;
            }
            None
        };

        let info = HeadingInfo {
            node_index,
            level,
            name: name.clone(),
            span: child.span,
            path: path.clone(),
            kind,
            mode,
        };
        let unit_id = UnitId {
            module: module.clone(),
            path,
        };
        index.units.push(UnitInfo {
            id: unit_id,
            level,
            name: name.clone(),
            span: child.span,
            kind,
            function,
        });
        if level == 1 && mode == SemanticMode::Exec {
            // H1 Exec: direct-body carriers before the next heading have no
            // owning H2 function. A Knowledge H2 body is inert (inline code
            // there is documentation), and Exec H3 sections are checked above.
            let mut scan = node_index + 1;
            while scan < doc.root.children.len() {
                if let MdKind::Heading { .. } = doc.root.children[scan].kind {
                    break;
                }
                if has_payload(&doc.root.children[scan]) {
                    index.diagnostics.push(Diagnostic::error(
                        codes::SEM_EXEC_WITHOUT_FUNCTION,
                        Some(doc.root.children[scan].span),
                        "executable payload has no owning Exec H2 function",
                    ));
                    break;
                }
                scan += 1;
            }
        }
        heading_infos.push(info.clone());
        stack.push((level, info.path.clone(), mode));
    }

    // Function body ranges and spans.
    for heading in heading_infos.iter().filter(|h| h.level == 2) {
        // A Knowledge H2 has no function range.
        if !index
            .units
            .iter()
            .any(|unit| unit.id.path == heading.path && unit.kind == UnitKind::Function)
        {
            continue;
        }
        let body_start = heading.node_index + 1;
        let body_end = doc.root.children[body_start..]
            .iter()
            .position(|child| match child.kind {
                MdKind::Heading { level, .. } => level <= 2,
                _ => false,
            })
            .map(|offset| body_start + offset)
            .unwrap_or(doc.root.children.len());
        let span = if body_end > body_start {
            let mut span = heading.span;
            for block in &doc.root.children[body_start..body_end] {
                span = span.merge(block.span);
            }
            span
        } else {
            heading.span
        };
        let unit = index
            .units
            .iter()
            .find(|unit| unit.id.path == heading.path && unit.kind == UnitKind::Function)
            .expect("function unit exists");
        let FunctionId { name, .. } = unit.function.clone().expect("function id");
        let local_name = name.rsplit('.').next().unwrap_or(&name).to_string();
        let namespace = if name.contains('.') {
            Some(name.rsplit_once('.').unwrap().0.to_string())
        } else {
            None
        };
        index.functions.push(FunctionScope {
            id: unit.function.clone().unwrap(),
            local_name,
            namespace,
            unit: unit.id.clone(),
            heading_span: heading.span,
            body_start,
            body_end,
            span,
        });
    }

    for heading in &heading_infos {
        let Some(unit) = index.units.iter().find(|unit| unit.id.path == heading.path) else {
            continue;
        };
        let slug = github_slug(&heading.name);
        if slug.is_empty() {
            continue;
        }
        let mut count = index.anchors.keys().filter(|key| *key == &slug).count();
        let key = if count == 0 {
            slug.clone()
        } else {
            loop {
                count += 1;
                let candidate = format!("{slug}-{count}");
                if !index.anchors.contains_key(&candidate) {
                    break candidate;
                }
            }
        };
        index.anchors.insert(key, unit.id.clone());
    }

    index.headings = heading_infos;
    index
        .diagnostics
        .dedup_by(|a, b| a.code == b.code && a.span == b.span && a.message == b.message);
    index
}

/// GitHub-style heading slug, kept in a separate namespace from canonical
/// unit names. Unicode letters/digits survive, spaces become `-`, other
/// punctuation is dropped and text is lowercased.
pub fn github_slug(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            for lower in ch.to_lowercase() {
                slug.push(lower);
            }
        } else if ch == ' ' || ch == '-' || ch == '\t' {
            slug.push('-');
        }
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_markdown::parse;

    fn index_of(source: &str) -> UnitIndex {
        let doc = parse(source);
        build_units(&doc, &ModuleId::from("index.mq.md"))
    }

    #[test]
    fn builds_function_ranges() {
        let index = index_of(
            "# @knowledge 命名空间\n\n## @exec 阶乘\n\n`x = 1`\n\n### 小节\n\n正文\n\n## @exec 下一个\n\n`return y`\n",
        );
        assert_eq!(index.functions.len(), 2);
        assert_eq!(index.functions[0].local_name, "阶乘");
        assert_eq!(index.functions[0].namespace.as_deref(), Some("命名空间"));
        assert_eq!(index.functions[0].id.name, "命名空间.阶乘");
        assert_eq!(index.functions[0].body_start, 2);
        // ### section stays inside the factorial function body.
        assert!(index.functions[0].body_end > index.functions[0].body_start + 2);
        assert_eq!(index.functions[1].local_name, "下一个");
    }

    #[test]
    fn root_level_function_without_namespace() {
        let index = index_of("## @exec main\n\n`打印(1)`\n");
        assert_eq!(index.functions.len(), 1);
        assert_eq!(index.functions[0].id.name, "main");
        assert!(index.functions[0].namespace.is_none());
        assert!(index.diagnostics.is_empty());
    }

    #[test]
    fn empty_heading_is_an_error() {
        let index = index_of("## @exec\n\n正文\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|d| d.code == "sem.empty_unit_name" && d.message.contains("empty")));
    }

    #[test]
    fn duplicate_function_names_are_errors() {
        let index = index_of("## @exec f\n\n## @exec f\n");
        let errors = index
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("duplicate"))
            .count();
        assert_eq!(errors, 1);
    }

    #[test]
    fn same_name_in_different_namespaces_is_allowed() {
        let index = index_of("# @knowledge a\n\n## @exec f\n\n# @knowledge b\n\n## @exec f\n");
        let duplicates = index
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("duplicate"))
            .count();
        assert_eq!(duplicates, 0);
        assert_eq!(index.functions.len(), 2);
        assert_eq!(index.functions[0].id.name, "a.f");
        assert_eq!(index.functions[1].id.name, "b.f");
    }

    #[test]
    fn executable_outside_function_warns_but_does_not_execute() {
        // RFC-LANG-0002: a Strong at the Knowledge root is inert prose with no
        // diagnostic; only an Exec H2 is a callable function.
        let index = index_of("**x = 1**\n\n## @exec f\n\n`return y`\n");
        assert!(index.diagnostics.is_empty());
        assert_eq!(index.functions.len(), 1);
    }
}
