//! Heading scope and semantic-mode index (RFC-LANG-0002).
//!
//! Markdown headings are sibling blocks, so scopes are derived from heading
//! levels, not from container nesting:
//!
//! * `heading(level=1)` -> namespace / top-level unit
//! * `heading(level=2, mode=Exec)` -> callable function
//! * `heading(level=2, mode=Knowledge)` -> inert knowledge unit
//! * `heading(level>=3)` -> section of the enclosing unit, never a callable
//!   function
//!
//! Each heading declares `@exec`/`@knowledge` or inherits the nearest ancestor
//! heading's mode; the document root is Knowledge. The range of a heading
//! extends to the next heading of the same or a lower level (same or smaller
//! number). Headings inside block quotes, list items or code fences cannot
//! appear as direct children of the document, hence they never create units.

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
    /// How the effective mode was established.
    pub mode_origin: ModeOrigin,
    /// Unit that provided the effective mode (this unit, an explicit ancestor,
    /// or the document root).
    pub mode_source_unit_id: UnitId,
    /// Nearest real ancestor unit (the root for a top-level heading).
    pub parent_id: UnitId,
    /// Complete section range: this heading through the next same-or-higher
    /// heading, or the end of the source. It includes child headings.
    pub full_span: Span,
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

    /// Complete public range of one unit: the whole source for the root, the
    /// heading-through-next-same-or-higher-heading range otherwise.
    pub fn unit_range(&self, id: &UnitId) -> Option<Span> {
        if id.path.is_empty() {
            return self
                .units
                .iter()
                .find(|unit| unit.id.path.is_empty())
                .map(|unit| unit.span);
        }
        self.headings
            .iter()
            .find(|heading| heading.path == id.path)
            .map(|heading| heading.full_span)
    }

    /// Innermost unit whose complete range contains `span`.
    ///
    /// Used for document links: the root covers the whole source, so a linked
    /// span always has an owner. Ties prefer the deeper path, then the later
    /// start (a child cannot be larger than its parent).
    pub fn unit_containing_span(&self, span: Span) -> Option<UnitId> {
        self.units
            .iter()
            .filter_map(|unit| {
                let range = self.unit_range(&unit.id)?;
                (range.start <= span.start && span.end <= range.end).then_some((unit, range))
            })
            .max_by_key(|(unit, range)| (unit.id.path.len(), range.start))
            .map(|(unit, _)| unit.id.clone())
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

/// Semantic mode declared or inherited by a heading (RFC-LANG-0002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMode {
    /// Document/knowledge region; no executable statements.
    Knowledge,
    /// Executable region; only an Exec H2 is callable.
    Exec,
}

/// How a heading's effective mode was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeOrigin {
    /// The document root is Knowledge by default.
    Default,
    /// The ATX heading declares `@exec` or `@knowledge`.
    Explicit,
    /// The heading inherits the mode of an ancestor or the root.
    Inherited,
}

impl ModeOrigin {
    /// Stable public name used by Analysis.
    pub fn as_str(self) -> &'static str {
        match self {
            ModeOrigin::Default => "default",
            ModeOrigin::Explicit => "explicit",
            ModeOrigin::Inherited => "inherited",
        }
    }
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
    let root_id = UnitId {
        module: module.clone(),
        path: Vec::new(),
    };
    index.units.push(UnitInfo {
        id: root_id.clone(),
        level: 0,
        name: String::new(),
        // The root covers the whole source, including a tolerated UTF-8 BOM.
        span: Span::new(0, doc.source.len()),
        kind: UnitKind::Root,
        function: None,
    });

    // Stack of (level, path, mode, mode_source_unit_id) for ancestors.
    let mut stack: Vec<(u8, Vec<String>, SemanticMode, UnitId)> = Vec::new();
    let mut seen: BTreeMap<(Vec<String>, String), Span> = BTreeMap::new();
    let mut current_namespace: Option<String> = None;
    let mut current_function: Option<FunctionId> = None;
    let mut heading_infos: Vec<HeadingInfo> = Vec::new();

    for (node_index, child) in doc.root.children.iter().enumerate() {
        let MdKind::Heading { level, setext } = &child.kind else {
            continue;
        };
        let level = *level;
        // RFC-LANG-0002: only ATX headings may carry a mode declaration; a
        // Setext heading simply inherits the nearest ancestor mode.
        let (declared, declared_name, declaration_error) = if *setext {
            (None, None, None)
        } else {
            heading_declaration(child)
        };
        if let Some(error) = declaration_error {
            index.diagnostics.push(error);
            if declared_name.is_none() {
                continue;
            }
        }
        // Resolve the ancestor scope before deriving the mode: remove every
        // same-or-higher-level heading first so a heading can only inherit from
        // its nearest *ancestor*, never from the previous sibling.
        while let Some((parent_level, _, _, _)) = stack.last() {
            if *parent_level >= level {
                stack.pop();
            } else {
                break;
            }
        }
        let inherited = stack
            .last()
            .map(|(_, _, mode, _)| *mode)
            .unwrap_or(SemanticMode::Knowledge);
        let inherited_source = stack
            .last()
            .map(|(_, _, _, source)| source.clone())
            .unwrap_or_else(|| root_id.clone());
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

        let parent_path = stack
            .last()
            .map(|(_, path, _, _)| path.clone())
            .unwrap_or_default();
        let parent_id = UnitId {
            module: module.clone(),
            path: parent_path.clone(),
        };
        let path = {
            let mut path = parent_path.clone();
            path.push(name.clone());
            path
        };
        let unit_id = UnitId {
            module: module.clone(),
            path: path.clone(),
        };
        let (mode_origin, mode_source_unit_id) = match declared {
            Some(_) => (ModeOrigin::Explicit, unit_id.clone()),
            None => (ModeOrigin::Inherited, inherited_source),
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
            // `.` separates namespace from local name in a FunctionId. A
            // component that itself contains `.` would make two different
            // definitions render the same public id; reject it explicitly
            // instead of silently producing a non-injective identity.
            let ambiguous = local.contains('.')
                || current_namespace
                    .as_deref()
                    .map(|namespace| namespace.contains('.'))
                    .unwrap_or(false);
            if ambiguous {
                index.diagnostics.push(
                    Diagnostic::error(
                        codes::SEM_AMBIGUOUS_IDENTITY,
                        Some(child.span),
                        "function or namespace name contains `.`, which is the namespace separator; the public function id would be ambiguous",
                    )
                    .with_hint(
                        "rename the namespace or function so it does not contain a dot",
                    ),
                );
            }
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
            mode_origin,
            mode_source_unit_id,
            parent_id,
            // Filled after all headings are known (the end is the next
            // same-or-higher heading or EOF).
            full_span: child.span,
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
        stack.push((
            level,
            info.path.clone(),
            mode,
            info.mode_source_unit_id.clone(),
        ));
    }

    // Complete section ranges: from this heading to the next heading of the
    // same or a higher level, or EOF. Child headings are included.
    let total = doc.source.len();
    for position in 0..heading_infos.len() {
        let level = heading_infos[position].level;
        let start = heading_infos[position].span.start;
        let end = heading_infos[position + 1..]
            .iter()
            .find(|candidate| candidate.level <= level)
            .map(|candidate| candidate.span.start)
            .unwrap_or(total);
        heading_infos[position].full_span = Span::new(start, end.max(start));
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
    fn setext_headings_inherit_and_never_declare_a_mode() {
        // `@exec f` is the level-2 heading text under a Setext underline; it is
        // not a declaration, so the H2 stays Knowledge and creates no function.
        let index = index_of("# @knowledge 项目\n\n@exec f\n---\n\n`return 1`\n");
        assert!(index.functions.is_empty(), "{:?}", index.functions);
        let heading = index
            .headings
            .iter()
            .find(|heading| heading.level == 2)
            .expect("level-2 heading");
        assert_eq!(heading.mode, SemanticMode::Knowledge);
        assert_eq!(heading.name, "@exec f");
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

    #[test]
    fn unmarked_sibling_inherits_ancestor_not_previous_exec_sibling() {
        // Regression for RFC-EXT-0003 H-06: the previous sibling is Exec but the
        // nearest ancestor is Knowledge, so 普通说明 must stay Knowledge.
        let index =
            index_of("# @knowledge 数学\n\n## @exec 运行\n\n`return 1`\n\n## 普通说明\n\n叙述。\n");
        assert_eq!(index.functions.len(), 1);
        assert_eq!(index.functions[0].id.name, "数学.运行");
        let sibling = index
            .headings
            .iter()
            .find(|heading| heading.name == "普通说明")
            .expect("sibling heading");
        assert_eq!(sibling.kind, UnitKind::Knowledge);
        assert_eq!(sibling.mode, SemanticMode::Knowledge);
    }

    #[test]
    fn unmarked_sibling_inherits_exec_ancestor_not_previous_knowledge_sibling() {
        let index =
            index_of("# @exec 项目\n\n## @knowledge 解释\n\n叙述。\n\n## 计算\n\n`return 1`\n");
        assert_eq!(index.diagnostics.len(), 0, "{:?}", index.diagnostics);
        assert_eq!(index.functions.len(), 1);
        assert_eq!(index.functions[0].id.name, "项目.计算");
        let sibling = index
            .headings
            .iter()
            .find(|heading| heading.name == "计算")
            .expect("sibling heading");
        assert_eq!(sibling.mode, SemanticMode::Exec);
        assert_eq!(sibling.kind, UnitKind::Function);
    }

    #[test]
    fn deep_sections_follow_their_own_ancestors() {
        // H3 under an Exec H2 is an Exec section; H3 under a Knowledge H2 is not
        // polluted by the earlier Exec chain.
        let index = index_of(
            "# @knowledge 数学\n\n## @exec 计算\n\n### 细节\n\n`return 1`\n\n## 说明\n\n### 例子\n\n叙述。\n",
        );
        let depth = |name: &str| {
            index
                .headings
                .iter()
                .find(|heading| heading.name == name)
                .unwrap_or_else(|| panic!("missing heading {name}"))
                .mode
        };
        assert_eq!(depth("细节"), SemanticMode::Exec);
        assert_eq!(depth("说明"), SemanticMode::Knowledge);
        assert_eq!(index.functions.len(), 1);
    }

    #[test]
    fn invalid_marker_does_not_corrupt_later_siblings() {
        let index = index_of("# @knowledge 数学\n\n## @Exec 坏标记\n\n## 正常\n\n`return 1`\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|d| d.code == "sem.invalid_mode_marker"));
        let normal = index
            .headings
            .iter()
            .find(|heading| heading.name == "正常")
            .expect("valid sibling heading");
        assert_eq!(normal.mode, SemanticMode::Knowledge);
        assert_eq!(normal.kind, UnitKind::Knowledge);
    }

    #[test]
    fn full_spans_cover_sections_and_parent_edges_form_a_tree() {
        let source = "# @knowledge 力学\n\n## @exec 计算力\n\n`return 1`\n\n### 细节\n\n正文\n\n## 说明\n\n结尾\n";
        let index = index_of(source);
        let by_name = |name: &str| {
            index
                .headings
                .iter()
                .find(|heading| heading.name == name)
                .unwrap_or_else(|| panic!("missing heading {name}"))
        };
        let calc = by_name("计算力");
        let detail = by_name("细节");
        let note = by_name("说明");
        // The function range includes its H3 child but not the next H2.
        assert_eq!(calc.full_span.start, calc.span.start);
        assert!(calc.full_span.end > detail.span.start);
        assert_eq!(calc.full_span.end, note.span.start);
        assert_eq!(detail.full_span.end, note.span.start);
        assert_eq!(note.full_span.end, source.len());
        assert_eq!(
            detail.parent_id.path,
            vec!["力学".to_string(), "计算力".to_string()]
        );
        assert_eq!(calc.parent_id.path, vec!["力学".to_string()]);
        let root = &index.units[0];
        assert_eq!(root.span, marqdo_span::Span::new(0, source.len()));
        assert_eq!(
            calc.parent_id,
            index
                .units
                .iter()
                .find(|u| u.name == "力学")
                .unwrap()
                .id
                .clone()
        );
        assert_eq!(by_name("力学").parent_id, root.id);
    }

    #[test]
    fn mode_origin_and_source_are_explicit() {
        let index = index_of(
            "# @knowledge 数学\n\n## @exec 计算\n\n### 细节\n\n`return 1`\n\n## 说明\n\n正文\n",
        );
        let by_name = |name: &str| {
            index
                .headings
                .iter()
                .find(|heading| heading.name == name)
                .unwrap_or_else(|| panic!("missing heading {name}"))
        };
        let namespace = by_name("数学");
        assert_eq!(namespace.mode_origin, ModeOrigin::Explicit);
        assert_eq!(
            namespace.mode_source_unit_id,
            namespace_unit(&index, "数学")
        );
        let calc = by_name("计算");
        assert_eq!(calc.mode_origin, ModeOrigin::Explicit);
        assert_eq!(
            calc.mode_source_unit_id.path,
            vec!["数学".to_string(), "计算".to_string()]
        );
        let detail = by_name("细节");
        assert_eq!(detail.mode_origin, ModeOrigin::Inherited);
        assert_eq!(
            detail.mode_source_unit_id.path,
            calc.mode_source_unit_id.path
        );
        let note = by_name("说明");
        assert_eq!(note.mode_origin, ModeOrigin::Inherited);
        assert_eq!(note.mode_source_unit_id.path, vec!["数学".to_string()]);
    }

    fn namespace_unit(index: &UnitIndex, name: &str) -> UnitId {
        index
            .units
            .iter()
            .find(|unit| unit.name == name)
            .map(|unit| unit.id.clone())
            .expect("namespace unit")
    }

    #[test]
    fn a_dot_in_a_function_component_is_rejected_as_an_ambiguous_identity() {
        let index = index_of("# @knowledge a.b\n\n## @exec f\n\n`return 1`\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "sem.ambiguous_identity"));
        let index = index_of("## @exec a.b\n\n`return 1`\n");
        assert!(index
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "sem.ambiguous_identity"));
    }

    #[test]
    fn innermost_unit_owns_a_linked_span() {
        let source = "前置说明。\n\n# @knowledge 数学\n\n## @exec 计算\n\n见[链接](#细节)。\n\n### 细节\n\n`return 1`\n";
        let index = index_of(source);
        let link_start = source.find("[链接]").unwrap();
        let owner = index
            .unit_containing_span(Span::new(link_start, link_start + 2))
            .expect("owning unit");
        assert_eq!(owner.path, vec!["数学".to_string(), "计算".to_string()]);
        let root = index.unit_containing_span(Span::new(0, 1)).expect("root");
        assert!(root.path.is_empty());
    }

    #[test]
    fn knowledge_scope_payloads_and_sections_stay_inert() {
        let index = index_of(
            "# @knowledge 数学\n\n$$\nA = \\pi r^2\n$$\n\n`数学表达式`\n\n## @knowledge 公式\n\n**不是函数**\n",
        );
        assert!(index.functions.is_empty(), "{:?}", index.functions);
        for heading in &index.headings {
            assert_eq!(heading.mode, SemanticMode::Knowledge);
        }
        assert!(index
            .units
            .iter()
            .all(|unit| !matches!(unit.kind, UnitKind::Function)));
    }
}
