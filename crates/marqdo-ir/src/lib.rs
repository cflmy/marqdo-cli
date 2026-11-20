//! Executable IR produced from the canonical Markdown AST.
//!
//! The IR is a *program* representation, never a second Markdown parser: it is
//! produced only by `marqdo-frontend` from `marqdo-markdown` nodes and carries
//! spans that point back into the original source. Prose nodes are deliberately
//! absent, so changing narrative text cannot change the IR (blueprint 14.2).
//!
//! Every executable construct keeps the source span of the Markdown node that
//! defined it, which is what diagnostics, `inspect --ir` and the interpreter
//! need. Values used at compile time (signature defaults) are explicit.

use std::collections::BTreeMap;
use std::fmt;

use marqdo_expression::{Arg, Expr};
use marqdo_span::Span;
use serde::{Deserialize, Serialize};

pub mod builtins;

/// Canonical identity of a module: project-relative path with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModuleId(pub String);

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ModuleId {
    fn from(value: &str) -> Self {
        ModuleId(value.to_string())
    }
}

/// Canonical identity of a function: module plus canonical function name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FunctionId {
    /// Owning module.
    pub module: ModuleId,
    /// Canonical (normalised) function name.
    pub name: String,
}

impl fmt::Display for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.module, self.name)
    }
}

/// Identity of an addressable heading range.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitId {
    /// Owning module.
    pub module: ModuleId,
    /// Canonical heading path, from the outermost heading to this unit.
    pub path: Vec<String>,
}

impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.module)?;
        for (index, part) in self.path.iter().enumerate() {
            if index > 0 {
                f.write_str("/")?;
            }
            f.write_str(part)?;
        }
        Ok(())
    }
}

/// Role of a heading range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    /// Document root.
    Root,
    /// Level-1 heading: namespace / object domain.
    Namespace,
    /// Level-2 heading: function.
    Function,
    /// Level-3+ heading inside a function: named section, not a callable unit.
    Section,
}

/// One addressable heading range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitInfo {
    /// Canonical unit identity.
    pub id: UnitId,
    /// Heading level (0 for the root).
    pub level: u8,
    /// Normalised heading name (empty for the root).
    pub name: String,
    /// Span of the heading node (empty span for the root).
    pub span: Span,
    /// Role of this unit.
    pub kind: UnitKind,
    /// Function defined by this unit, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionId>,
}

/// How a function's parameter list was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind {
    /// Declared by a signature paragraph after the heading.
    Explicit,
    /// Inferred from free variable reads by data-flow analysis.
    Inferred,
}

/// One function parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Span of the parameter occurrence (signature or first read).
    pub span: Span,
    /// Optional type annotation text (explicit signatures only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
    /// Compile-time constant default expression, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Expr>,
}

/// A function definition mapped from a level-2 heading range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    /// Canonical function identity (namespace-qualified when namespaced).
    pub id: FunctionId,
    /// Function name as written in its own namespace.
    pub local_name: String,
    /// Enclosing level-1 namespace, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Addressable unit defined by the heading.
    pub unit: UnitId,
    /// Heading span.
    pub heading_span: Span,
    /// Declaration span (heading through end of range).
    pub span: Span,
    /// Parameters in canonical (sorted) order for inferred signatures, in
    /// declaration order for explicit ones.
    pub params: Vec<Param>,
    /// Whether the signature was explicit or inferred.
    pub signature: SignatureKind,
    /// Function body in executable order.
    pub body: Vec<Stmt>,
}

/// An executable statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stmt", rename_all = "snake_case")]
pub enum Stmt {
    /// Local assignment (`strong` node with `name = expr`).
    Assign {
        /// Target name.
        name: String,
        /// Span of the target name (approximated by the payload span).
        name_span: Span,
        /// Value expression.
        value: Expr,
        /// Span of the whole statement.
        span: Span,
    },
    /// Expression statement, e.g. a call whose result is discarded.
    Eval {
        /// Expression to evaluate.
        expr: Expr,
        /// Span of the whole statement.
        span: Span,
    },
    /// Markdown-link call form from a block quote.
    Call {
        /// Resolved call site.
        call: CallSite,
        /// Span of the whole statement.
        span: Span,
    },
    /// Ordered list: first matching condition wins.
    Branch {
        /// Arms in AST order.
        arms: Vec<BranchArm>,
        /// Span of the list node.
        span: Span,
    },
    /// Bullet list item: a `while` loop.
    Loop {
        /// Loop condition.
        condition: Expr,
        /// Span of the condition payload.
        condition_span: Span,
        /// Loop body.
        body: Vec<Stmt>,
        /// Span of the list item.
        span: Span,
    },
    /// Return from the enclosing function (`emph` node).
    Return {
        /// Returned expression.
        value: Expr,
        /// Span of the whole return node.
        span: Span,
    },
}

impl Stmt {
    /// Span of the statement.
    pub fn span(&self) -> Span {
        match self {
            Stmt::Assign { span, .. }
            | Stmt::Eval { span, .. }
            | Stmt::Call { span, .. }
            | Stmt::Branch { span, .. }
            | Stmt::Loop { span, .. }
            | Stmt::Return { span, .. } => *span,
        }
    }
}

/// One arm of a branch (ordered list item).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchArm {
    /// Boolean condition expression.
    pub condition: Expr,
    /// Span of the condition code span.
    pub condition_span: Span,
    /// Statements executed when the condition is true.
    pub body: Vec<Stmt>,
    /// Span of the whole list item.
    pub span: Span,
}

/// Target of a call statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum CallTarget {
    /// Unqualified function in the current module.
    Function {
        /// Canonical function name.
        name: String,
    },
    /// Function reached through an imported module alias.
    Qualified {
        /// Local import alias.
        module_alias: String,
        /// Canonical function name.
        function: String,
    },
    /// Function resolved from a Markdown link target.
    Resolved {
        /// Canonical module identity.
        module: ModuleId,
        /// Canonical function name.
        function: String,
    },
}

/// A call statement with evaluated arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallSite {
    /// Callee target.
    pub target: CallTarget,
    /// Arguments in source order.
    pub args: Vec<Arg>,
    /// Span of the call.
    pub span: Span,
}

/// One explicit import declared in front matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDecl {
    /// Local alias used in expressions (`geometry.圆面积(...)`).
    pub alias: String,
    /// Declared module address as written.
    pub target: String,
    /// Span of the front matter.
    pub span: Span,
}

/// An inert document resource addressable through `ref`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resource", rename_all = "snake_case")]
pub enum Resource {
    /// A math formula inside a unit range.
    Formula {
        /// Unit that contains the formula.
        unit: UnitId,
        /// Raw LaTeX without delimiters.
        latex: String,
        /// Display math flag.
        display: bool,
        /// Source span.
        span: Span,
    },
    /// A fenced code block inside a unit range.
    Code {
        /// Unit that contains the code block.
        unit: UnitId,
        /// Fence language (empty for plain fences).
        language: String,
        /// Fence source.
        source: String,
        /// SHA-256 of the source, for content integrity checks.
        content_hash: String,
        /// Source span.
        span: Span,
    },
}

/// A compiled module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    /// Canonical module identity.
    pub id: ModuleId,
    /// Addressable units in document order.
    pub units: Vec<UnitInfo>,
    /// Functions in document order.
    pub functions: Vec<Function>,
    /// Explicit imports.
    pub imports: Vec<ImportDecl>,
    /// Inert resources (formulas, code fences) in document order.
    pub resources: Vec<Resource>,
    /// GitHub-style anchors for `ref("#...")` lookups. Anchors are a separate
    /// namespace from canonical unit ids.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub anchors: BTreeMap<String, UnitId>,
}

impl Module {
    /// Find a function by canonical qualified name, local name or (when unique)
    /// local name across namespaces.
    pub fn function(&self, name: &str) -> Option<&Function> {
        if let Some(found) = self.functions.iter().find(|f| f.id.name == name) {
            return Some(found);
        }
        let mut matches = self.functions.iter().filter(|f| f.local_name == name);
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    /// Find a function declared in a specific namespace.
    pub fn namespaced_function(&self, namespace: &str, name: &str) -> Option<&Function> {
        self.functions
            .iter()
            .find(|f| f.namespace.as_deref() == Some(namespace) && f.local_name == name)
    }

    /// All namespace names declared by level-1 headings.
    pub fn namespaces(&self) -> impl Iterator<Item = &str> {
        self.units.iter().filter_map(|unit| match unit.kind {
            UnitKind::Namespace => Some(unit.name.as_str()),
            _ => None,
        })
    }

    /// The default entry function, if defined.
    pub fn entry(&self) -> Option<&Function> {
        self.function("main")
    }

    /// Find an import declaration by alias.
    pub fn import(&self, alias: &str) -> Option<&ImportDecl> {
        self.imports.iter().find(|import| import.alias == alias)
    }

    /// All resources contained by a unit.
    pub fn resources_of<'a>(&'a self, unit: &UnitId) -> impl Iterator<Item = &'a Resource> + 'a {
        let unit = unit.clone();
        self.resources
            .iter()
            .filter(move |resource| match resource {
                Resource::Formula { unit: owner, .. } | Resource::Code { unit: owner, .. } => {
                    owner == &unit
                }
            })
    }
}

/// One resolved import edge from the module loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedImport {
    /// Importing module.
    pub from: ModuleId,
    /// Local alias.
    pub alias: String,
    /// Imported module identity.
    pub to: ModuleId,
}

/// A whole program: modules keyed by canonical id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// Modules keyed by canonical module id.
    pub modules: BTreeMap<ModuleId, Module>,
    /// Canonical id of the module being run.
    pub main_module: Option<ModuleId>,
    /// Resolved import edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_imports: Vec<ResolvedImport>,
}

impl Program {
    /// Insert a module.
    pub fn insert(&mut self, module: Module) {
        self.modules.insert(module.id.clone(), module);
    }

    /// Get a module.
    pub fn module(&self, id: &ModuleId) -> Option<&Module> {
        self.modules.get(id)
    }

    /// Find a function by full identity.
    pub fn function(&self, id: &FunctionId) -> Option<&Function> {
        self.modules.get(&id.module)?.function(&id.name)
    }

    /// Resolve an import alias to a module.
    pub fn resolve_import(&self, from: &ModuleId, alias: &str) -> Option<&ModuleId> {
        self.resolved_imports
            .iter()
            .find(|edge| &edge.from == from && edge.alias == alias)
            .map(|edge| &edge.to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_expression::parse_expression;

    #[test]
    fn stmt_keeps_spans() {
        let expr = parse_expression("1 + 2").unwrap();
        let stmt = Stmt::Return {
            value: expr,
            span: Span::new(4, 12),
        };
        assert_eq!(stmt.span(), Span::new(4, 12));
    }

    #[test]
    fn module_lookup_helpers() {
        let module = Module {
            id: ModuleId::from("index.mq.md"),
            units: vec![],
            functions: vec![],
            imports: vec![ImportDecl {
                alias: "geometry".into(),
                target: "@geometry".into(),
                span: Span::new(0, 0),
            }],
            resources: vec![],
            anchors: BTreeMap::new(),
        };
        assert!(module.entry().is_none());
        assert_eq!(module.import("geometry").unwrap().target, "@geometry");
        assert_eq!(module.id.to_string(), "index.mq.md");
    }

    #[test]
    fn ir_serializes_for_inspect() {
        let stmt = Stmt::Return {
            value: parse_expression("x ** 2").unwrap(),
            span: Span::new(0, 6),
        };
        let json = serde_json::to_value(&stmt).unwrap();
        assert_eq!(json["stmt"], "return");
        assert_eq!(json["span"]["start"], 0);
    }
}
