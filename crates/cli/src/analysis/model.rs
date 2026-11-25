//! Typed public Analysis DTO (spec/analysis.md).
//!
//! These types are the boundary model, not a dump of internal IR: every field
//! is projected explicitly from the prepared program. The canonical encoder
//! serializes this model once; the file writer and the Observation digest use
//! the same bytes.

use serde::Serialize;
use serde_json::Value;

/// Half-open UTF-8 byte span in the owning module's `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpanJson {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl From<marqdo_span::Span> for SpanJson {
    fn from(span: marqdo_span::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

/// Root Analysis document.
#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    /// `marqdo.analysis`.
    pub schema: &'static str,
    /// Current revision.
    pub schema_version: u32,
    /// Producer identity.
    pub producer: Producer,
    /// Project snapshot and entry context.
    pub project: Project,
    /// Loaded modules, sorted by id.
    pub modules: Vec<ModuleAnalysis>,
    /// Deterministic diagnostics.
    pub diagnostics: Vec<DiagnosticRecord>,
}

/// Producer identity.
#[derive(Debug, Clone, Serialize)]
pub struct Producer {
    /// Always `marqdo`.
    pub name: &'static str,
    /// Product version.
    pub version: &'static str,
    /// Markdown profile id.
    pub markdown_profile: &'static str,
}

/// Project snapshot identity.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    /// Logical root id.
    pub root_id: String,
    /// `manifest` or `workspace_snapshot`.
    pub identity_kind: &'static str,
    /// Module loaded from the CLI entry.
    pub entry_module_id: String,
    /// Captured lock digest, empty when no lock exists.
    pub lock_digest: String,
    /// Captured manifest digest, empty when no manifest exists.
    pub manifest_digest: String,
    /// Verified stdlib identity, if any.
    pub stdlib: Option<Stdlib>,
    /// Static extension identities that participated in preparation.
    pub extensions: Vec<Extension>,
}

/// Verified bundled stdlib identity.
#[derive(Debug, Clone, Serialize)]
pub struct Stdlib {
    /// Declared version.
    pub version: String,
    /// Verified content digest.
    pub digest: String,
    /// Package instance id.
    pub instance_id: String,
}

/// One static extension identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extension {
    /// `plugin`, `kernel` or `provider`.
    pub kind: &'static str,
    /// Declared package/provider name.
    pub name: String,
    /// Declared version.
    pub version: String,
    /// Logical owner package instance.
    pub logical_owner: String,
    /// Declared activation source, or `--plugin`.
    pub source: String,
    /// Manifest digest (`sha256:<hex>`).
    pub manifest_digest: String,
    /// Selected artifact digest (`sha256:<hex>`).
    pub artifact_digest: String,
    /// Selected host target triple.
    pub selected_target: String,
}

/// One loaded module.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleAnalysis {
    /// Canonical module id.
    pub id: String,
    /// Package instance.
    pub package_instance_id: String,
    /// Package-relative path with `/` separators.
    pub relative_path: String,
    /// SHA-256 of the exact source bytes.
    pub source_digest: String,
    /// Exact UTF-8 source text.
    pub source: String,
    /// Public Markdown AST.
    pub document: Value,
    /// Heading units in document order.
    pub units: Vec<UnitRecord>,
    /// Anchor map sorted by anchor.
    pub anchors: Vec<AnchorRecord>,
    /// Exec H2 functions in document order.
    pub functions: Vec<FunctionRecord>,
    /// Explicit imports in document order.
    pub imports: Vec<ImportRecord>,
    /// Markdown references in source order.
    pub references: Vec<ReferenceRecord>,
    /// Static call bindings in source order.
    pub calls: Vec<CallRecord>,
    /// Inert resources in document order.
    pub resources: Vec<ResourceRecord>,
}

/// One addressable heading range.
#[derive(Debug, Clone, Serialize)]
pub struct UnitRecord {
    /// Canonical unit id.
    pub id: String,
    /// Structured canonical path.
    pub path: Vec<String>,
    /// Parent unit id, root `null`.
    pub parent_id: Option<String>,
    /// `root`, `namespace`, `function`, `knowledge` or `section`.
    pub kind: &'static str,
    /// Normalised heading name.
    pub name: String,
    /// Heading level, root 0.
    pub level: u8,
    /// Effective mode.
    pub mode: &'static str,
    /// `default`, `explicit` or `inherited`.
    pub mode_origin: &'static str,
    /// Unit that provided the effective mode.
    pub mode_source_unit_id: String,
    /// Original heading span, root `null`.
    pub heading_span: Option<SpanJson>,
    /// Complete section range.
    pub span: SpanJson,
    /// Owning function id.
    pub function: Option<String>,
}

/// One anchor entry.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorRecord {
    /// GitHub-style anchor.
    pub anchor: String,
    /// Addressable unit.
    pub unit_id: String,
}

/// One Exec H2 function.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionRecord {
    /// Canonical function id.
    pub id: String,
    /// Addressable unit.
    pub unit_id: String,
    /// Local function name.
    pub local_name: String,
    /// Enclosing namespace.
    pub namespace: Option<String>,
    /// Declaration span.
    pub span: SpanJson,
    /// Heading span.
    pub heading_span: SpanJson,
    /// Signature.
    pub signature: SignatureRecord,
    /// Conservative effects.
    pub effects: Vec<&'static str>,
    /// Statements.
    pub body: Vec<Value>,
}

/// Function signature.
#[derive(Debug, Clone, Serialize)]
pub struct SignatureRecord {
    /// `explicit` or `inferred`.
    pub kind: &'static str,
    /// Parameters.
    pub params: Vec<ParamRecord>,
}

/// One function parameter.
#[derive(Debug, Clone, Serialize)]
pub struct ParamRecord {
    /// Parameter name.
    pub name: String,
    /// Whether no default value exists.
    pub required: bool,
    /// Declared type annotation.
    pub r#type: Option<String>,
    /// Default expression.
    pub default: Option<Value>,
    /// Declaration span.
    pub span: SpanJson,
}

/// One explicit import.
#[derive(Debug, Clone, Serialize)]
pub struct ImportRecord {
    /// Local alias.
    pub alias: String,
    /// Declared target.
    pub target: String,
    /// Declaration span.
    pub span: SpanJson,
    /// Resolved module id.
    pub resolved: Option<String>,
}

/// One resolved document reference.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceRecord {
    /// Source module.
    pub source: String,
    /// Innermost owning unit.
    pub source_unit: String,
    /// Raw URL.
    pub url: String,
    /// Link span.
    pub span: SpanJson,
    /// Resolver state.
    pub state: &'static str,
    /// `module`, `file` or `null`.
    pub target_kind: Option<&'static str>,
    /// Resolved module.
    pub target_module: Option<String>,
    /// Resolved unit.
    pub target_unit: Option<String>,
    /// Logical candidate package instance.
    pub candidate_package_instance_id: Option<String>,
    /// Normalised package-relative candidate.
    pub candidate_path: Option<String>,
    /// Decoded fragment.
    pub fragment: Option<String>,
    /// Ambiguous exact candidates.
    pub candidates: Vec<String>,
}

/// One static call record.
#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    /// Caller function id.
    pub function: String,
    /// Call expression span.
    pub span: SpanJson,
    /// Callee shape.
    pub callee: Value,
    /// Function, builtin, plugin or unresolved.
    pub state: &'static str,
    /// Bound target value.
    pub target: Option<String>,
    /// `function`, `builtin`, `plugin` or `null`.
    pub target_kind: Option<&'static str>,
    /// Static plugin owner, if any.
    pub target_owner: Option<TargetOwner>,
    /// `dynamic`, `unbound` or `null`.
    pub reason: Option<&'static str>,
}

/// Static plugin owner of a bound export.
#[derive(Debug, Clone, Serialize)]
pub struct TargetOwner {
    /// Always `plugin`.
    pub kind: &'static str,
    /// Plugin package.
    pub package: String,
    /// Verified artifact digest.
    pub digest: String,
}

/// One inert resource.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceRecord {
    /// Module-local ordinal id.
    pub id: String,
    /// Owning module.
    pub module_id: String,
    /// Innermost owning unit.
    pub unit_id: String,
    /// `formula` or `code`.
    pub kind: &'static str,
    /// AST node span.
    pub span: SpanJson,
    /// Payload digest.
    pub content_digest: String,
    /// Code fence language.
    pub language: Option<String>,
    /// Formula display mode (`null` for code).
    pub display: Option<bool>,
}

/// One diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticRecord {
    /// Owning module.
    pub module: String,
    /// Package-relative path.
    pub relative_path: String,
    /// Primary span.
    pub span: Option<SpanJson>,
    /// 1-based line.
    pub line: Option<usize>,
    /// 1-based column.
    pub column: Option<usize>,
    /// `error` or `warning`.
    pub severity: &'static str,
    /// Stable code.
    pub code: String,
    /// Message.
    pub message: String,
    /// Hints.
    pub hints: Vec<String>,
}
