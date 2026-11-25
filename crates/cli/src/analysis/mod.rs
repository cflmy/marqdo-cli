//! Public Analysis projection (spec/analysis.md, ADR-0069).
//!
//! One typed projection is built from the same `project_prepare::prepare`
//! facts consumed by `check`, `inspect`, `run` and `call`; the canonical
//! encoder is shared by the file writer and the Observation digest. The
//! exporter is read-only and never executes a document, opens an extension or
//! reads a link target outside the explicitly prepared catalog.

mod document;
mod model;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use marqdo_expression::{BinaryOp, Expr, UnaryOp};
use marqdo_ir::{Function, Module, ModuleId, PlaceTarget, Resource, SignatureKind, Stmt, UnitKind};
use marqdo_span::Span;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::document_refs::{self, ReferenceMode, TargetKind};
use crate::loader::{FileDiagnostic, LoadedProject};
use crate::project_prepare::PreparedProgram;

pub use model::*;

/// Schema name and version.
pub const SCHEMA: &str = "marqdo.analysis";
/// Current public revision.
pub const SCHEMA_VERSION: u32 = 3;
/// Markdown profile recorded by the producer.
const MARKDOWN_PROFILE: &str = "marqdo.md.v1";

/// Projection shared with `inspect --ast`.
pub fn document_projection(root: &marqdo_markdown::MdNode, source_len: usize) -> Value {
    document::document_json(root, source_len)
}

/// Projection shared with `inspect --refs`.
pub fn reference_projection(reference: document_refs::Reference) -> ReferenceRecord {
    reference_record(reference)
}

/// Projection shared with `inspect --units`.
pub fn units_projection(index: &marqdo_frontend::UnitIndex, source_len: usize) -> Vec<UnitRecord> {
    units_json(index, source_len)
}

/// Build and semantically validate the deterministic public projection.
pub fn build(prepared: &PreparedProgram) -> Result<Analysis, String> {
    let loaded = &prepared.loaded;
    let references = document_refs::references_for_project(loaded, ReferenceMode::LoadedOnly);
    let mut modules = Vec::with_capacity(loaded.files.len());
    for (module_id, file) in &loaded.files {
        let source = &file.source;
        let unit_index = &file.analysis.units;
        let module = &file.analysis.module;
        let references = references
            .by_module
            .get(module_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(reference_record)
            .collect();
        modules.push(ModuleAnalysis {
            id: module_id.to_string(),
            package_instance_id: module_id.instance.clone(),
            relative_path: module_id.relative_path.clone(),
            source_digest: sha256_hex(source.as_bytes()),
            source: source.clone(),
            document: document::document_json(&file.analysis.document.root, source.len()),
            units: units_json(unit_index, source.len()),
            anchors: anchors_json(unit_index),
            functions: module.functions.iter().map(function_record).collect(),
            imports: module
                .imports
                .iter()
                .map(|import| ImportRecord {
                    alias: import.alias.clone(),
                    target: import.target.clone(),
                    span: import.span.into(),
                    resolved: loaded
                        .program
                        .resolve_import(module_id, &import.alias)
                        .map(ToString::to_string),
                })
                .collect(),
            references,
            calls: calls_json(loaded, module),
            resources: resources_json(module_id, module),
        });
    }
    modules.sort_by(|left, right| left.id.cmp(&right.id));

    let analysis = Analysis {
        schema: SCHEMA,
        schema_version: SCHEMA_VERSION,
        producer: Producer {
            name: "marqdo",
            version: env!("CARGO_PKG_VERSION"),
            markdown_profile: MARKDOWN_PROFILE,
        },
        project: Project {
            root_id: loaded.identity.root_id(),
            identity_kind: loaded.identity.kind(),
            entry_module_id: loaded.entry.to_string(),
            lock_digest: loaded.lock_digest.clone(),
            manifest_digest: loaded.manifest_digest.clone(),
            stdlib: loaded.stdlib.as_ref().map(|stdlib| Stdlib {
                version: stdlib.version.clone(),
                digest: stdlib.digest.clone(),
                instance_id: stdlib.instance_id.clone(),
            }),
            extensions: extensions_json(prepared),
        },
        modules,
        diagnostics: diagnostics_json(&loaded.diagnostics),
    };
    validate(&analysis)?;
    Ok(analysis)
}

/// Encode exactly the bytes written by `check --emit`.
pub fn encode(prepared: &PreparedProgram) -> Result<Vec<u8>, String> {
    let analysis = build(prepared)?;
    serde_json::to_vec_pretty(&analysis).map_err(|error| format!("cannot encode analysis: {error}"))
}

/// SHA-256 of the exact emitted Analysis bytes.
pub fn digest(prepared: &PreparedProgram) -> Result<String, String> {
    encode(prepared).map(|bytes| format!("sha256:{}", sha256_hex(&bytes)))
}

/// Atomically write the projection through an owner-only temporary file.
///
/// An error never leaves a partial artifact and never touches an input source.
pub fn write(prepared: &PreparedProgram, path: &Path) -> Result<(), String> {
    let bytes = encode(prepared)?;
    let parent = output_parent(path);
    std::fs::create_dir_all(&parent)
        .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
    ensure_safe_output(prepared, path)?;
    let temporary = create_temporary(&parent, path)?;
    let write_result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("cannot create `{}`: {error}", temporary.display()))?;
        restrict_permissions(&file)?;
        file.write_all(&bytes)
            .map_err(|error| format!("cannot write `{}`: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot flush `{}`: {error}", temporary.display()))?;
        drop(file);
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("cannot publish `{}`: {error}", path.display()))
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result
}

/// Remove a previous success artifact after a failed check.
///
/// The same output guards apply: an input source, manifest, lock file, a
/// directory or a symlink is never removed.
pub fn discard(prepared: &PreparedProgram, path: &Path) -> Result<(), String> {
    ensure_safe_output(prepared, path)?;
    remove_if_present(path)
}

/// Remove a stale output when preparation itself failed (no PreparedProgram).
///
/// The entry source is explicitly protected even when it is the requested
/// output path, and a directory/symlink destination is refused.
pub fn discard_without_prepare(entry: Option<&Path>, path: &Path) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot inspect `{}`: {error}", path.display())),
    };
    if let Some(metadata) = &metadata {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to remove `{}`: it is a symbolic link",
                path.display()
            ));
        }
        if metadata.is_dir() {
            return Err(format!(
                "refusing to remove `{}`: it is a directory",
                path.display()
            ));
        }
    }
    if let Some(entry) = entry {
        if let (Ok(entry), Ok(target)) = (entry.canonicalize(), path.canonicalize()) {
            if entry == target {
                return Err(format!(
                    "refusing to remove `{}`: it is the input document",
                    path.display()
                ));
            }
        }
    }
    remove_if_present(path)
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot remove `{}`: {error}", path.display())),
    }
}

fn output_parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temporary(parent: &Path, path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "analysis.json".to_string());
    for _ in 0..64 {
        let nonce = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
        match std::fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => continue,
            Err(error) => return Err(format!("cannot inspect temporary path: {error}")),
        }
    }
    Err("cannot create a unique temporary file for the Analysis output".to_string())
}

#[cfg(unix)]
fn restrict_permissions(file: &std::fs::File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot restrict Analysis permissions: {error}"))
}

#[cfg(not(unix))]
fn restrict_permissions(_file: &std::fs::File) -> Result<(), String> {
    Ok(())
}

/// Refuse to overwrite a loaded source, manifest, lock file, a hard link to
/// one, a directory or a symlink at the destination.
fn ensure_safe_output(prepared: &PreparedProgram, path: &Path) -> Result<(), String> {
    let loaded = &prepared.loaded;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot inspect `{}`: {error}", path.display())),
    };
    if let Some(metadata) = &metadata {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to write `{}`: the destination is a symbolic link",
                path.display()
            ));
        }
        if metadata.is_dir() {
            return Err(format!(
                "refusing to write `{}`: the destination is a directory",
                path.display()
            ));
        }
    }
    let parent = output_parent(path);
    let parent = parent.canonicalize().unwrap_or_else(|_| parent.clone());
    let target = parent.join(
        path.file_name()
            .ok_or_else(|| format!("`{}` has no file name", path.display()))?,
    );
    let mut protected: Vec<PathBuf> = loaded
        .files
        .values()
        .map(|file| file.path.clone())
        .collect();
    protected.push(loaded.root.join(marqdo_package::MANIFEST_FILE));
    protected.push(loaded.root.join(marqdo_package::LOCK_FILE));
    for source in protected {
        let canonical = source.canonicalize().unwrap_or_else(|_| source.clone());
        if canonical == target {
            return Err(format!(
                "refusing to write `{}`: it is an input source, manifest or lock file",
                path.display()
            ));
        }
        if let Some(existing) = &metadata {
            if is_hard_link_to(existing, &source) {
                return Err(format!(
                    "refusing to write `{}`: it is a hard link to an input source",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_hard_link_to(metadata: &std::fs::Metadata, source: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(source_metadata) = std::fs::metadata(source) else {
        return false;
    };
    metadata.dev() == source_metadata.dev() && metadata.ino() == source_metadata.ino()
}

#[cfg(not(unix))]
fn is_hard_link_to(_metadata: &std::fs::Metadata, _source: &Path) -> bool {
    false
}

fn units_json(index: &marqdo_frontend::UnitIndex, source_len: usize) -> Vec<UnitRecord> {
    let mut units = Vec::with_capacity(index.units.len());
    for unit in &index.units {
        let heading = index
            .headings
            .iter()
            .find(|heading| heading.path == unit.id.path);
        let (mode, mode_origin, mode_source, heading_span, span) = match heading {
            Some(heading) => (
                mode_name(heading.mode),
                heading.mode_origin.as_str(),
                heading.mode_source_unit_id.to_string(),
                Some(heading.span.into()),
                heading.full_span.into(),
            ),
            None => (
                "knowledge",
                "default",
                unit.id.to_string(),
                None,
                Span::new(0, source_len).into(),
            ),
        };
        units.push(UnitRecord {
            id: unit.id.to_string(),
            path: unit.id.path.clone(),
            parent_id: heading.map(|heading| heading.parent_id.to_string()),
            kind: unit_kind_name(unit.kind),
            name: unit.name.clone(),
            level: unit.level,
            mode,
            mode_origin,
            mode_source_unit_id: mode_source,
            heading_span,
            span,
            function: unit.function.as_ref().map(ToString::to_string),
        });
    }
    units
}

fn anchors_json(index: &marqdo_frontend::UnitIndex) -> Vec<AnchorRecord> {
    index
        .anchors
        .iter()
        .map(|(anchor, unit)| AnchorRecord {
            anchor: anchor.clone(),
            unit_id: unit.to_string(),
        })
        .collect()
}

fn unit_kind_name(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Root => "root",
        UnitKind::Namespace => "namespace",
        UnitKind::Function => "function",
        UnitKind::Knowledge => "knowledge",
        UnitKind::Section => "section",
    }
}

fn mode_name(mode: marqdo_frontend::units::SemanticMode) -> &'static str {
    match mode {
        marqdo_frontend::units::SemanticMode::Knowledge => "knowledge",
        marqdo_frontend::units::SemanticMode::Exec => "exec",
    }
}

fn reference_record(reference: document_refs::Reference) -> ReferenceRecord {
    ReferenceRecord {
        source: reference.source.to_string(),
        source_unit: reference.source_unit.to_string(),
        url: reference.url,
        span: reference.span.into(),
        state: reference.state.as_str(),
        target_kind: reference.target_kind.map(TargetKind::as_str),
        target_module: reference.target_module.map(|module| module.to_string()),
        target_unit: reference.target_unit.map(|unit| unit.to_string()),
        candidate_package_instance_id: reference.candidate_package_instance_id,
        candidate_path: reference.candidate_path,
        fragment: reference.fragment,
        candidates: reference
            .candidates
            .into_iter()
            .map(|unit| unit.to_string())
            .collect(),
    }
}

fn function_record(function: &Function) -> FunctionRecord {
    FunctionRecord {
        id: function.id.to_string(),
        unit_id: function.unit.to_string(),
        local_name: function.local_name.clone(),
        namespace: function.namespace.clone(),
        span: function.span.into(),
        heading_span: function.heading_span.into(),
        signature: SignatureRecord {
            kind: match function.signature {
                SignatureKind::Explicit => "explicit",
                SignatureKind::Inferred => "inferred",
            },
            params: function
                .params
                .iter()
                .map(|param| ParamRecord {
                    name: param.name.clone(),
                    required: param.default.is_none(),
                    r#type: param.type_annotation.clone(),
                    default: param.default.as_ref().map(expr_json),
                    span: param.span.into(),
                })
                .collect(),
        },
        effects: vec!["unknown"],
        body: function.body.iter().map(stmt_json).collect(),
    }
}

fn resources_json(module_id: &ModuleId, module: &Module) -> Vec<ResourceRecord> {
    let mut formula_index = 0usize;
    let mut code_index = 0usize;
    module
        .resources
        .iter()
        .map(|resource| match resource {
            Resource::Formula {
                unit,
                latex,
                display,
                span,
            } => {
                formula_index += 1;
                ResourceRecord {
                    id: format!("{module_id}#resource:formula-{formula_index}"),
                    module_id: module_id.to_string(),
                    unit_id: unit.to_string(),
                    kind: "formula",
                    span: (*span).into(),
                    content_digest: sha256_hex(latex.as_bytes()),
                    language: None,
                    display: Some(*display),
                }
            }
            Resource::Code {
                unit,
                language,
                source: _,
                content_hash,
                span,
            } => {
                code_index += 1;
                ResourceRecord {
                    id: format!("{module_id}#resource:code-{code_index}"),
                    module_id: module_id.to_string(),
                    unit_id: unit.to_string(),
                    kind: "code",
                    span: (*span).into(),
                    content_digest: content_hash.clone(),
                    language: Some(language.clone()),
                    display: None,
                }
            }
        })
        .collect()
}

fn diagnostics_json(diagnostics: &[FileDiagnostic]) -> Vec<DiagnosticRecord> {
    diagnostics
        .iter()
        .map(|item| {
            let (line, column) = item
                .line_col()
                .map(|(line, column)| (Some(line), Some(column)))
                .unwrap_or((None, None));
            DiagnosticRecord {
                module: item.module_id.to_string(),
                relative_path: item.module_id.relative_path.clone(),
                span: item.diagnostic.span.map(Into::into),
                line,
                column,
                severity: if item.diagnostic.is_error() {
                    "error"
                } else {
                    "warning"
                },
                code: item.diagnostic.code.clone(),
                message: item.diagnostic.message.clone(),
                hints: item.diagnostic.hints.clone(),
            }
        })
        .collect()
}

fn extensions_json(prepared: &PreparedProgram) -> Vec<Extension> {
    let mut extensions: Vec<Extension> = prepared
        .extension_identities
        .iter()
        .map(|identity| Extension {
            kind: identity.kind.as_str(),
            name: identity.name.clone(),
            version: identity.version.clone(),
            logical_owner: identity.logical_owner.clone(),
            source: identity.source.clone(),
            manifest_digest: wire_digest(&identity.manifest_digest),
            artifact_digest: wire_digest(&identity.artifact_digest),
            selected_target: identity.selected_target.clone(),
        })
        .collect();
    for plugin in &prepared.plugins.plugins {
        extensions.push(Extension {
            kind: "plugin",
            name: plugin.package.clone(),
            version: plugin.version.clone(),
            logical_owner: "cli".to_string(),
            source: "--plugin".to_string(),
            manifest_digest: wire_digest(&plugin.manifest_sha256),
            artifact_digest: wire_digest(&plugin.artifact_sha256),
            selected_target: plugin.artifact_target.clone(),
        });
    }
    extensions.sort_by(|left, right| {
        (
            left.kind,
            &left.name,
            &left.logical_owner,
            &left.manifest_digest,
        )
            .cmp(&(
                right.kind,
                &right.name,
                &right.logical_owner,
                &right.manifest_digest,
            ))
    });
    extensions.dedup();
    extensions
}

fn wire_digest(raw: &str) -> String {
    if raw.starts_with("sha256:") {
        raw.to_string()
    } else {
        format!("sha256:{raw}")
    }
}

fn place_json(place: &PlaceTarget) -> Value {
    match place {
        PlaceTarget::Local { name, .. } => Value::String(name.clone()),
        PlaceTarget::Index { base, index, span } => json!({
            "kind": "index",
            "base": expr_json(base),
            "index": expr_json(index),
            "span": span_json(*span),
        }),
    }
}

fn stmt_json(stmt: &Stmt) -> Value {
    match stmt {
        Stmt::Assign {
            target,
            value,
            span,
            ..
        } => json!({
            "kind": "assign",
            "target": place_json(target),
            "value": expr_json(value),
            "span": span_json(*span),
        }),
        Stmt::Eval { expr, span } => json!({
            "kind": "expression",
            "value": expr_json(expr),
            "span": span_json(*span),
        }),
        Stmt::Branch { arms, span } => json!({
            "kind": "branch",
            "arms": arms.iter().map(|arm| json!({
                "condition": expr_json(&arm.condition),
                "condition_span": span_json(arm.condition_span),
                "body": arm.body.iter().map(stmt_json).collect::<Vec<_>>(),
                "span": span_json(arm.span),
            })).collect::<Vec<_>>(),
            "span": span_json(*span),
        }),
        Stmt::Loop {
            condition,
            condition_span,
            body,
            span,
        } => json!({
            "kind": "loop",
            "condition": expr_json(condition),
            "condition_span": span_json(*condition_span),
            "body": body.iter().map(stmt_json).collect::<Vec<_>>(),
            "span": span_json(*span),
        }),
        Stmt::Break { span } => json!({"kind": "break", "span": span_json(*span)}),
        Stmt::Continue { span } => json!({"kind": "continue", "span": span_json(*span)}),
        Stmt::Return { value, span } => json!({
            "kind": "return",
            "value": expr_json(value),
            "span": span_json(*span),
        }),
    }
}

fn expr_json(expr: &Expr) -> Value {
    let span = span_json(expr.span());
    match expr {
        Expr::Int(value) => {
            json!({"kind":"literal","type":"int","value":value.to_string(),"span":span})
        }
        Expr::Float(value) => json!({"kind":"literal","type":"float","value":value,"span":span}),
        Expr::Text(value) => json!({"kind":"literal","type":"text","value":value,"span":span}),
        Expr::Bool(value) => json!({"kind":"literal","type":"bool","value":value,"span":span}),
        Expr::None => json!({"kind":"literal","type":"none","span":span}),
        Expr::Ident(ident) => {
            json!({"kind":"variable","name":ident.name,"span":span_json(ident.span)})
        }
        Expr::List(items) => {
            json!({"kind":"list","items":items.iter().map(expr_json).collect::<Vec<_>>(),"span":span})
        }
        Expr::Record(entries) => json!({
            "kind":"record",
            "entries": entries.iter().map(|entry| json!({
                "key": entry.key,
                "key_span": span_json(entry.key_span),
                "value": expr_json(&entry.value),
            })).collect::<Vec<_>>(),
            "span":span
        }),
        Expr::Unary { op, operand, .. } => json!({
            "kind":"unary",
            "op": unary_name(*op),
            "operand": expr_json(operand),
            "span":span
        }),
        Expr::Binary {
            op, left, right, ..
        } => json!({
            "kind":"binary",
            "op": binary_name(*op),
            "left": expr_json(left),
            "right": expr_json(right),
            "span":span
        }),
        Expr::Member { base, name, .. } => json!({
            "kind":"member",
            "base": expr_json(base),
            "field": name,
            "span":span
        }),
        Expr::Index { base, index, .. } => json!({
            "kind":"index",
            "base": expr_json(base),
            "index": expr_json(index),
            "span":span
        }),
        Expr::Call { callee, args, .. } => json!({
            "kind":"call",
            "callee": expr_json(callee),
            "args": args.iter().map(arg_json).collect::<Vec<_>>(),
            "span":span
        }),
    }
}

fn arg_json(arg: &marqdo_expression::Arg) -> Value {
    json!({
        "name": arg.name,
        "value": expr_json(&arg.value),
        "span": span_json(arg.span),
    })
}

fn calls_json(project: &LoadedProject, module: &Module) -> Vec<CallRecord> {
    let mut calls = Vec::new();
    for function in &module.functions {
        collect_calls(project, function, &function.body, &mut calls);
    }
    calls.sort_by_key(|call| call.span.start);
    calls
}

fn collect_calls(
    project: &LoadedProject,
    function: &Function,
    body: &[Stmt],
    calls: &mut Vec<CallRecord>,
) {
    for stmt in body {
        match stmt {
            Stmt::Assign { target, value, .. } => {
                if let PlaceTarget::Index { base, index, .. } = target {
                    expr_calls(project, function, base, calls);
                    expr_calls(project, function, index, calls);
                }
                expr_calls(project, function, value, calls);
            }
            Stmt::Eval { expr: value, .. } => expr_calls(project, function, value, calls),
            Stmt::Return { value, .. } => expr_calls(project, function, value, calls),
            Stmt::Branch { arms, .. } => {
                for arm in arms {
                    expr_calls(project, function, &arm.condition, calls);
                    collect_calls(project, function, &arm.body, calls);
                }
            }
            Stmt::Loop {
                condition, body, ..
            } => {
                expr_calls(project, function, condition, calls);
                collect_calls(project, function, body, calls);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }
}

fn expr_calls(
    project: &LoadedProject,
    function: &Function,
    expr: &Expr,
    calls: &mut Vec<CallRecord>,
) {
    match expr {
        Expr::Call { callee, args, span } => {
            let key = marqdo_ir::call_binding::CallSiteKey::new(function.id.clone(), *span);
            let binding = project.program.call_bindings.get(&key);
            let (state, target, target_kind, target_owner, reason) = match binding {
                Some(marqdo_ir::call_binding::BoundTarget::Builtin { canonical_name }) => (
                    "builtin",
                    Some(canonical_name.clone()),
                    Some("builtin"),
                    None,
                    None,
                ),
                Some(marqdo_ir::call_binding::BoundTarget::Marqdo { function }) => (
                    "function",
                    Some(function.to_string()),
                    Some("function"),
                    None,
                    None,
                ),
                Some(marqdo_ir::call_binding::BoundTarget::AbiPlugin {
                    package,
                    export,
                    digest,
                    ..
                }) => (
                    "plugin",
                    Some(export.clone()),
                    Some("plugin"),
                    Some(TargetOwner {
                        kind: "plugin",
                        package: package.clone(),
                        digest: wire_digest(digest),
                    }),
                    None,
                ),
                Some(marqdo_ir::call_binding::BoundTarget::DynamicValue) => {
                    ("unresolved", None, None, None, Some("dynamic"))
                }
                None => ("unresolved", None, None, None, Some("unbound")),
            };
            calls.push(CallRecord {
                function: function.id.to_string(),
                span: (*span).into(),
                callee: callee_json(callee),
                state,
                target,
                target_kind,
                target_owner,
                reason,
            });
            for arg in args {
                expr_calls(project, function, &arg.value, calls);
            }
        }
        Expr::Unary { operand, .. } => expr_calls(project, function, operand, calls),
        Expr::Binary { left, right, .. } => {
            expr_calls(project, function, left, calls);
            expr_calls(project, function, right, calls);
        }
        Expr::List(items) => {
            for item in items {
                expr_calls(project, function, item, calls);
            }
        }
        Expr::Record(entries) => {
            for entry in entries {
                expr_calls(project, function, &entry.value, calls);
            }
        }
        Expr::Member { base, .. } => expr_calls(project, function, base, calls),
        Expr::Index { base, index, .. } => {
            expr_calls(project, function, base, calls);
            expr_calls(project, function, index, calls);
        }
        _ => {}
    }
}

fn callee_json(callee: &Expr) -> Value {
    match callee {
        Expr::Ident(ident) => json!({"kind":"ident","name":ident.name}),
        Expr::Member { base, name, .. } => {
            json!({"kind":"member","base":callee_json(base),"field":name})
        }
        other => json!({"kind":"expression","span":span_json(other.span())}),
    }
}

fn span_json(span: Span) -> Value {
    json!({"start": span.start, "end": span.end})
}

fn unary_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "not",
        UnaryOp::Neg => "neg",
        UnaryOp::Pos => "pos",
    }
}

fn binary_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::FloorDiv => "floor_div",
        BinaryOp::Mod => "mod",
        BinaryOp::Pow => "pow",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Check the cross-record invariants that JSON Schema cannot express.
fn validate(analysis: &Analysis) -> Result<(), String> {
    if analysis.schema != SCHEMA || analysis.schema_version != SCHEMA_VERSION {
        return Err("analysis schema/revision mismatch".to_string());
    }
    if analysis.producer.name != "marqdo" || analysis.producer.markdown_profile != MARKDOWN_PROFILE
    {
        return Err("analysis producer identity mismatch".to_string());
    }
    if !analysis.project.lock_digest.is_empty()
        && !analysis.project.lock_digest.starts_with("sha256:")
    {
        return Err("project.lock_digest must be empty or sha256:<hex>".to_string());
    }
    if !analysis.project.manifest_digest.is_empty()
        && !analysis.project.manifest_digest.starts_with("sha256:")
    {
        return Err("project.manifest_digest must be empty or sha256:<hex>".to_string());
    }
    let mut module_ids = BTreeSet::new();
    let mut sources: BTreeMap<&str, &str> = BTreeMap::new();
    let mut module_units: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut function_ids: BTreeSet<String> = BTreeSet::new();
    let mut previous = None;
    for module in &analysis.modules {
        if let Some(previous) = previous {
            if previous >= module.id.as_str() {
                return Err(format!(
                    "modules are not strictly sorted by id: `{previous}` then `{}`",
                    module.id
                ));
            }
        }
        previous = Some(module.id.as_str());
        if !module_ids.insert(module.id.as_str()) {
            return Err(format!("duplicate module id `{}`", module.id));
        }
        if module.source_digest != sha256_hex(module.source.as_bytes()) {
            return Err(format!("source_digest mismatch in `{}`", module.id));
        }
        sources.insert(module.id.as_str(), module.source.as_str());
        let mut units = BTreeSet::new();
        for unit in &module.units {
            if !units.insert(unit.id.clone()) {
                return Err(format!("duplicate unit id `{}`", unit.id));
            }
        }
        module_units.insert(module.id.as_str(), units);
        for function in &module.functions {
            if !function_ids.insert(function.id.clone()) {
                return Err(format!("duplicate function id `{}`", function.id));
            }
        }
    }
    if !module_ids.contains(analysis.project.entry_module_id.as_str()) {
        return Err("project.entry_module_id does not name a module".to_string());
    }
    for module in &analysis.modules {
        let source = module.source.as_str();
        validate_span(
            module.document.get("span"),
            source.len(),
            source,
            &format!("{}.document", module.id),
        )?;
        validate_ast(&module.document, source.len(), source, &module.id)?;
        if module.document.get("kind").and_then(Value::as_str) != Some("document") {
            return Err(format!("{} document root has wrong kind", module.id));
        }
        let mut ast_literals: BTreeMap<(usize, usize), (String, String)> = BTreeMap::new();
        collect_ast_literals(&module.document, &mut ast_literals);
        let root_span = module.document.get("span").cloned().unwrap_or_default();
        if root_span.get("start").and_then(Value::as_u64) != Some(0)
            || root_span.get("end").and_then(Value::as_u64) != Some(source.len() as u64)
        {
            return Err(format!(
                "{} document root does not cover the source",
                module.id
            ));
        }

        let mut unit_by_id: BTreeMap<&str, &UnitRecord> = BTreeMap::new();
        for unit in &module.units {
            unit_by_id.insert(unit.id.as_str(), unit);
            if unit.id != unit_id_string(&module.id, &unit.path) {
                return Err(format!("unit id/path mismatch in `{}`", module.id));
            }
            validate_span(
                Some(&json!({"start": unit.span.start, "end": unit.span.end})),
                source.len(),
                source,
                &format!("{}.units.{}", module.id, unit.id),
            )?;
            if let Some(heading) = &unit.heading_span {
                validate_span(
                    Some(&json!({"start": heading.start, "end": heading.end})),
                    source.len(),
                    source,
                    &format!("{}.heading_span", unit.id),
                )?;
            }
            if unit.level == 0 {
                if unit.parent_id.is_some() || !unit.path.is_empty() {
                    return Err(format!("root unit `{}` has a parent or path", unit.id));
                }
            } else {
                let parent = unit
                    .parent_id
                    .as_deref()
                    .and_then(|parent| unit_by_id.get(parent))
                    .ok_or_else(|| format!("unit `{}` has a missing parent", unit.id))?;
                let mut expected = parent.path.clone();
                expected.push(unit.name.clone());
                if expected != unit.path {
                    return Err(format!("unit `{}` path does not match its parent", unit.id));
                }
                if unit.span.start > parent.span.end || unit.span.end < parent.span.start {
                    return Err(format!("unit `{}` escapes its parent span", unit.id));
                }
            }
        }
        // Mode source must be the unit itself or an ancestor.
        for unit in &module.units {
            let mut current = Some(unit);
            let mut found = false;
            while let Some(candidate) = current {
                if candidate.id == unit.mode_source_unit_id {
                    found = true;
                    break;
                }
                current = candidate
                    .parent_id
                    .as_deref()
                    .and_then(|parent| unit_by_id.get(parent).copied());
            }
            if !found {
                return Err(format!(
                    "unit `{}` mode_source_unit_id is not an ancestor",
                    unit.id
                ));
            }
        }
        let mut previous_anchor: Option<&str> = None;
        for anchor in &module.anchors {
            if let Some(previous) = previous_anchor {
                if previous >= anchor.anchor.as_str() {
                    return Err(format!("anchors are not sorted in `{}`", module.id));
                }
            }
            previous_anchor = Some(&anchor.anchor);
            if !unit_by_id.contains_key(anchor.unit_id.as_str()) {
                return Err(format!("anchor `{}` names a missing unit", anchor.anchor));
            }
        }
        let mut unit_function: BTreeMap<String, String> = BTreeMap::new();
        for unit in &module.units {
            if let Some(function) = &unit.function {
                if let Some(existing) = unit_function.insert(unit.id.clone(), function.clone()) {
                    if &existing != function {
                        return Err(format!("unit `{}` has two functions", unit.id));
                    }
                }
                if unit.function.as_deref().is_some_and(|function| {
                    !module
                        .functions
                        .iter()
                        .any(|candidate| candidate.id == function)
                }) {
                    return Err(format!("unit `{}` names a missing function", unit.id));
                }
            }
        }
        for function in &module.functions {
            if !unit_by_id.contains_key(function.unit_id.as_str()) {
                return Err(format!("function `{}` names a missing unit", function.id));
            }
            let unit = unit_by_id[function.unit_id.as_str()];
            if unit.kind != "function" {
                return Err(format!(
                    "function `{}` is not attached to an Exec H2 unit",
                    function.id
                ));
            }
            if unit.function.as_deref() != Some(function.id.as_str()) {
                return Err(format!("function/unit link mismatch for `{}`", function.id));
            }
            validate_span(
                Some(&json!({"start": function.span.start, "end": function.span.end})),
                source.len(),
                source,
                &format!("{}.span", function.id),
            )?;
            validate_span(
                Some(
                    &json!({"start": function.heading_span.start, "end": function.heading_span.end}),
                ),
                source.len(),
                source,
                &format!("{}.heading_span", function.id),
            )?;
            for statement in &function.body {
                validate_json_spans(statement, source.len(), source, &function.id)?;
            }
        }
        for import in &module.imports {
            validate_span(
                Some(&json!({"start": import.span.start, "end": import.span.end})),
                source.len(),
                source,
                &format!("{}.import", module.id),
            )?;
            if let Some(resolved) = &import.resolved {
                if !module_ids.contains(resolved.as_str()) {
                    return Err(format!(
                        "import `{}` resolves to a module outside the artifact",
                        import.target
                    ));
                }
            }
        }
        for reference in &module.references {
            validate_span(
                Some(&json!({"start": reference.span.start, "end": reference.span.end})),
                source.len(),
                source,
                &format!("{}.reference", module.id),
            )?;
            if !unit_by_id.contains_key(reference.source_unit.as_str()) {
                return Err(format!(
                    "reference `{}` names a missing source unit",
                    reference.url
                ));
            }
            validate_reference(reference, &module_ids, &module_units)?;
        }
        let mut call_spans = BTreeSet::new();
        for call in &module.calls {
            validate_span(
                Some(&json!({"start": call.span.start, "end": call.span.end})),
                source.len(),
                source,
                &format!("{}.call", module.id),
            )?;
            if !call_spans.insert((call.function.clone(), call.span.start, call.span.end)) {
                return Err(format!("duplicate call record at {}", call.span.start));
            }
            if !function_ids.contains(&call.function) {
                return Err(format!("call names a missing function `{}`", call.function));
            }
            match call.state {
                "function" if call.target_kind == Some("function") => {
                    if let Some(target) = &call.target {
                        if !function_ids.contains(target) {
                            return Err(format!("call target `{target}` does not exist"));
                        }
                    } else {
                        return Err("resolved function call has no target".to_string());
                    }
                }
                "builtin" if call.target_kind == Some("builtin") => {}
                "plugin" => {
                    if call.target_kind != Some("plugin") || call.target_owner.is_none() {
                        return Err("plugin call lacks its static owner".to_string());
                    }
                }
                "unresolved" => {
                    if call.target.is_some() || call.target_kind.is_some() {
                        return Err("unresolved call carries a target".to_string());
                    }
                }
                _ => return Err(format!("invalid call state `{}`", call.state)),
            }
        }
        let mut resource_ids = BTreeSet::new();
        for resource in &module.resources {
            if !resource_ids.insert(&resource.id) {
                return Err(format!("duplicate resource id `{}`", resource.id));
            }
            if resource.module_id != module.id {
                return Err("resource module mismatch".to_string());
            }
            if !unit_by_id.contains_key(resource.unit_id.as_str()) {
                return Err(format!("resource `{}` names a missing unit", resource.id));
            }
            validate_span(
                Some(&json!({"start": resource.span.start, "end": resource.span.end})),
                source.len(),
                source,
                &resource.id,
            )?;
            if resource.content_digest.len() != 64 {
                return Err(format!("resource `{}` has a bad digest", resource.id));
            }
            match resource.kind {
                "formula" if resource.display.is_none() => {
                    return Err("formula resource has no display flag".to_string());
                }
                "code" if resource.display.is_some() => {
                    return Err("code resource has a display flag".to_string());
                }
                "formula" | "code" => {}
                _ => return Err("unknown resource kind".to_string()),
            }
            let Some((node_kind, literal)) =
                ast_literals.get(&(resource.span.start, resource.span.end))
            else {
                return Err(format!(
                    "resource `{}` span does not name an AST payload node",
                    resource.id
                ));
            };
            let expected = if resource.kind == "formula" {
                node_kind == "math" || node_kind == "math_block"
            } else {
                node_kind == "code_block"
            };
            if !expected {
                return Err(format!(
                    "resource `{}` kind does not match its AST node",
                    resource.id
                ));
            }
            if resource.content_digest != sha256_hex(literal.as_bytes()) {
                return Err(format!(
                    "resource `{}` content_digest does not match the AST payload",
                    resource.id
                ));
            }
        }
    }
    let module_ids: BTreeSet<&str> = analysis
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    for diagnostic in &analysis.diagnostics {
        if !module_ids.contains(diagnostic.module.as_str()) {
            return Err(format!(
                "diagnostic names a missing module `{}`",
                diagnostic.module
            ));
        }
        if let Some(span) = &diagnostic.span {
            if span.start > span.end {
                return Err("diagnostic span is reversed".to_string());
            }
        }
        if diagnostic.severity == "error" {
            return Err(format!(
                "successful Analysis contains error diagnostic `{}`",
                diagnostic.code
            ));
        }
    }
    Ok(())
}

fn unit_id_string(module_id: &str, path: &[String]) -> String {
    let mut out = format!("{module_id}#");
    for (index, segment) in path.iter().enumerate() {
        if index > 0 {
            out.push('/');
        }
        out.push_str(&marqdo_ir::escape_identity_segment(segment));
    }
    out
}

fn validate_reference(
    reference: &ReferenceRecord,
    modules: &BTreeSet<&str>,
    module_units: &BTreeMap<&str, BTreeSet<String>>,
) -> Result<(), String> {
    match reference.state {
        "resolved" => match reference.target_kind {
            Some("module") => {
                let target = reference
                    .target_module
                    .as_deref()
                    .ok_or_else(|| "resolved module reference has no target module".to_string())?;
                if !modules.contains(target) {
                    return Err(format!(
                        "resolved reference names missing module `{target}`"
                    ));
                }
                if let Some(unit) = &reference.target_unit {
                    let units = module_units
                        .get(target)
                        .ok_or_else(|| "missing target unit table".to_string())?;
                    if !units.contains(unit) {
                        return Err(format!("resolved reference names missing unit `{unit}`"));
                    }
                }
            }
            Some("file") => {
                if reference.target_module.is_some() || reference.target_unit.is_some() {
                    return Err("file reference must not name a module/unit".to_string());
                }
            }
            _ => return Err("resolved reference has no target kind".to_string()),
        },
        "not_loaded" | "external" | "outside_scope" | "invalid" => {
            if reference.target_module.is_some()
                || reference.target_unit.is_some()
                || reference.target_kind.is_some()
            {
                return Err(format!(
                    "reference state `{}` carries a target",
                    reference.state
                ));
            }
        }
        "missing" => {
            if reference.target_unit.is_some() {
                return Err("missing reference carries a target unit".to_string());
            }
        }
        "ambiguous" => {
            if reference.target_unit.is_some() || reference.candidates.len() < 2 {
                return Err("ambiguous reference must carry several candidates".to_string());
            }
        }
        other => return Err(format!("unknown reference state `{other}`")),
    }
    Ok(())
}

fn collect_ast_literals(node: &Value, index: &mut BTreeMap<(usize, usize), (String, String)>) {
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "math" | "math_block" | "code_block") {
        if let (Some(start), Some(end), Some(literal)) = (
            node.get("span")
                .and_then(|span| span.get("start"))
                .and_then(Value::as_u64),
            node.get("span")
                .and_then(|span| span.get("end"))
                .and_then(Value::as_u64),
            node.get("literal").and_then(Value::as_str),
        ) {
            index.insert(
                (start as usize, end as usize),
                (kind.to_string(), literal.to_string()),
            );
        }
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            collect_ast_literals(child, index);
        }
    }
}

fn validate_span(
    span: Option<&Value>,
    source_len: usize,
    source: &str,
    owner: &str,
) -> Result<(), String> {
    let Some(span) = span else {
        return Ok(());
    };
    let Some(start) = span.get("start").and_then(Value::as_u64) else {
        return Err(format!("{owner} span has no integer start"));
    };
    let Some(end) = span.get("end").and_then(Value::as_u64) else {
        return Err(format!("{owner} span has no integer end"));
    };
    let (start, end) = (start as usize, end as usize);
    if start > end || end > source_len {
        return Err(format!(
            "{owner} span {start}..{end} is outside 0..{source_len}"
        ));
    }
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(format!("{owner} span is not on UTF-8 boundaries"));
    }
    Ok(())
}

fn validate_ast(node: &Value, source_len: usize, source: &str, module: &str) -> Result<(), String> {
    let Some(kind) = node.get("kind").and_then(Value::as_str) else {
        return Err(format!("{module} AST node has no kind"));
    };
    if kind.starts_with("footnote") {
        return Err(format!("{module} AST contains a disabled `{kind}` node"));
    }
    validate_span(
        node.get("span"),
        source_len,
        source,
        &format!("{module}.document.{kind}"),
    )?;
    let children = node
        .get("children")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{module} AST `{kind}` has no children array"))?;
    if matches!(
        kind,
        "text"
            | "code"
            | "code_block"
            | "math"
            | "math_block"
            | "html_block"
            | "html_inline"
            | "front_matter"
            | "thematic_break"
            | "soft_break"
            | "line_break"
    ) && !children.is_empty()
    {
        return Err(format!("{module} AST leaf `{kind}` has children"));
    }
    for child in children {
        validate_ast(child, source_len, source, module)?;
    }
    Ok(())
}

fn validate_json_spans(
    value: &Value,
    source_len: usize,
    source: &str,
    owner: &str,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            if let Some(span) = object.get("span") {
                validate_span(Some(span), source_len, source, owner)?;
            }
            for nested in object.values() {
                validate_json_spans(nested, source_len, source, owner)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_json_spans(item, source_len, source, owner)?;
            }
        }
        _ => {}
    }
    Ok(())
}
