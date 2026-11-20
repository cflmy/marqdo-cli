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
use marqdo_markdown::{normalize_name, plain_text, MarkdownDocument, MdKind};
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

    // Stack of (level, path) for ancestor headings with smaller level.
    let mut stack: Vec<(u8, Vec<String>)> = Vec::new();
    let mut seen: BTreeMap<(Vec<String>, String), Span> = BTreeMap::new();
    let mut current_namespace: Option<String> = None;
    let mut current_function: Option<FunctionId> = None;
    let mut heading_infos: Vec<HeadingInfo> = Vec::new();

    for (node_index, child) in doc.root.children.iter().enumerate() {
        let MdKind::Heading { level, .. } = &child.kind else {
            continue;
        };
        let level = *level;
        let raw_name = normalize_name(&plain_text(child));
        let Some(name) = raw_name else {
            index.diagnostics.push(
                Diagnostic::error(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
                    Some(child.span),
                    "heading name must not be empty",
                )
                .with_hint("give the heading a non-empty canonical name"),
            );
            continue;
        };

        while let Some((parent_level, _)) = stack.last() {
            if *parent_level >= level {
                stack.pop();
            } else {
                break;
            }
        }
        let parent_path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        let path = {
            let mut path = parent_path.clone();
            path.push(name.clone());
            path
        };

        // Duplicate canonical names in the same scope are a hard error; never
        // silently rename with -1/-2 suffixes.
        let key = (parent_path.clone(), name.clone());
        if let Some(previous) = seen.get(&key) {
            let previous = *previous;
            index.diagnostics.push(
                Diagnostic::error(
                    codes::MARKDOWN_INVALID_CONSTRUCT,
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
            2 => UnitKind::Function,
            _ => UnitKind::Section,
        };
        if level == 1 {
            current_namespace = Some(name.clone());
            current_function = None;
        }
        let function = if level == 2 {
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
        } else if level >= 3 {
            current_function.clone()
        } else {
            None
        };

        let info = HeadingInfo {
            node_index,
            level,
            name: name.clone(),
            span: child.span,
            path: path.clone(),
            kind,
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
        heading_infos.push(info.clone());
        stack.push((level, info.path.clone()));
    }

    // Function body ranges and spans.
    for heading in heading_infos.iter().filter(|h| h.level == 2) {
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
        let local_name = heading.name.clone();
        let namespace = match heading.path.len() {
            0 => None,
            _ if heading.path.len() == 1 => None,
            _ => Some(heading.path[0].clone()),
        };
        let namespace = namespace.filter(|_| {
            index
                .units
                .iter()
                .any(|unit| unit.kind == UnitKind::Namespace && unit.name == heading.path[0])
        });
        let qualified = match &namespace {
            Some(namespace) => format!("{namespace}.{local_name}"),
            None => local_name.clone(),
        };
        index.functions.push(FunctionScope {
            id: FunctionId {
                module: module.clone(),
                name: qualified,
            },
            local_name,
            namespace,
            unit: UnitId {
                module: module.clone(),
                path: heading.path.clone(),
            },
            heading_span: heading.span,
            body_start,
            body_end,
            span,
        });
    }

    // Warnings for would-be execution nodes outside every function body.
    for (node_index, child) in doc.root.children.iter().enumerate() {
        if index.function_containing_block(node_index).is_some() {
            continue;
        }
        let mut warn = |span: Span, what: &str| {
            index.diagnostics.push(
                Diagnostic::warning(
                    codes::MARKDOWN_INERT_EXECUTABLE,
                    Some(span),
                    format!("{what} outside a function body is inert document content"),
                )
                .with_hint("move it under a level-2 heading (`## 函数名`) to execute it"),
            );
        };
        match &child.kind {
            MdKind::BlockQuote => warn(child.span, "a block quote invocation"),
            MdKind::List { .. } => warn(child.span, "a list control structure"),
            MdKind::Paragraph => {
                let mut found: Option<(Span, &'static str)> = None;
                child.walk(&mut |node| {
                    if found.is_none() {
                        match node.kind {
                            MdKind::Strong => found = Some((node.span, "an execution statement")),
                            MdKind::Emph => found = Some((node.span, "a return statement")),
                            _ => {}
                        }
                    }
                });
                if let Some((span, what)) = found {
                    warn(span, what);
                }
            }
            _ => {}
        }
    }

    // Derive GitHub-style anchors for navigation and `ref("#...")`. Anchors are
    // not identity: they may collide and receive -1/-2 suffixes exactly like
    // GitHub's slugger, while internal ids stay canonical.
    let mut slug_counts: BTreeMap<String, usize> = BTreeMap::new();
    for unit in &index.units {
        if unit.level == 0 {
            continue;
        }
        let base = github_slug(&unit.name);
        if base.is_empty() {
            continue;
        }
        let count = slug_counts.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        index.anchors.insert(slug, unit.id.clone());
    }

    index.headings = heading_infos;
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
            "# 命名空间\n\n## 阶乘\n\n**x = 1**\n\n### 小节\n\n正文\n\n## 下一个\n\n*y*\n",
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
        let index = index_of("## main\n\n> 打印(1)\n");
        assert_eq!(index.functions.len(), 1);
        assert_eq!(index.functions[0].id.name, "main");
        assert!(index.functions[0].namespace.is_none());
        assert!(index.diagnostics.is_empty());
    }

    #[test]
    fn empty_heading_is_an_error() {
        let index = index_of("##   \n\n正文\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MARKDOWN_INVALID_CONSTRUCT && d.message.contains("empty")));
    }

    #[test]
    fn duplicate_function_names_are_errors() {
        let index = index_of("## f\n\n## f\n");
        let errors = index
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("duplicate"))
            .count();
        assert_eq!(errors, 1);
    }

    #[test]
    fn same_name_in_different_namespaces_is_allowed() {
        let index = index_of("# a\n\n## f\n\n# b\n\n## f\n");
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
        let index = index_of("**x = 1**\n\n## f\n\n*y*\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MARKDOWN_INERT_EXECUTABLE));
        assert_eq!(index.functions.len(), 1);
    }
}
