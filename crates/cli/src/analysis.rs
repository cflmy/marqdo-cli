//! Public Analysis v2 projection (spec/analysis-v2.md).
//!
//! This is deliberately not a serde dump of `marqdo-ir`: the schema and node
//! kinds are the public contract, while internal Rust types may change. The
//! exporter is read-only and never executes a document, loads a plugin or
//! starts a foreign kernel.

use std::path::Path;

use marqdo_expression::{BinaryOp, Expr, UnaryOp};
use marqdo_ir::{CallTarget, Function, Module, ModuleId, Resource, SignatureKind, Stmt, UnitKind};
use marqdo_span::Span;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::loader::LoadedProject;

/// Schema name and version.
pub const SCHEMA: &str = "marqdo.analysis";
pub const SCHEMA_VERSION: u32 = 2;

/// Build the deterministic public projection.
pub fn build(project: &LoadedProject) -> Value {
    let mut modules = Vec::new();
    for (module_id, file) in &project.files {
        let module = &file.analysis.module;
        modules.push(json!({
            "id": module_id.to_string(),
            "package_instance_id": module_id.instance,
            "relative_path": module_id.relative_path,
            "source_digest": sha256_hex(file.source.as_bytes()),
            "units": units_json(module),
            "functions": module.functions.iter().map(function_json).collect::<Vec<_>>(),
            "imports": module.imports.iter().map(|import| json!({
                "alias": import.alias,
                "target": import.target,
                "span": import.span,
                "resolved": project.program.resolve_import(module_id, &import.alias).map(ToString::to_string),
            })).collect::<Vec<_>>(),
            "references": reference_edges(project, module_id),
            "calls": module.functions.iter().flat_map(|function| calls_json(project, module, function)).collect::<Vec<_>>(),
            "resources": resources_json(module_id, module),
        }));
    }
    modules.sort_by(|a, b| {
        a.get("id")
            .and_then(Value::as_str)
            .cmp(&b.get("id").and_then(Value::as_str))
    });
    json!({
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "producer": {
            "name": "marqdo",
            "version": env!("CARGO_PKG_VERSION"),
            "markdown_profile": "marqdo.md.v1",
        },
        "project": {
            "root_id": project.identity.root_id(),
            "identity_kind": project.identity.kind(),
            "lock_digest": lock_digest(project),
        },
        "modules": modules,
        "diagnostics": diagnostics_json(project),
    })
}

/// Encode exactly the bytes written by `check --emit`.
fn encode(project: &LoadedProject) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&build(project))
        .map_err(|error| format!("cannot encode analysis: {error}"))
}

/// SHA-256 of the exact emitted Analysis bytes (Trace binds to this digest).
pub fn digest(project: &LoadedProject) -> String {
    encode(project)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_else(|_| sha256_hex(b""))
}

/// Atomically write the projection. An error must not leave a stale success
/// artifact behind.
pub fn write(project: &LoadedProject, path: &Path) -> Result<(), String> {
    let bytes = encode(project)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "analysis.json".to_string()),
        std::process::id()
    ));
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("cannot write `{}`: {error}", temporary.display()))?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot publish `{}`: {error}", path.display()));
    }
    Ok(())
}

fn lock_digest(project: &LoadedProject) -> Value {
    let path = project.root.join(marqdo_package::LOCK_FILE);
    match std::fs::read(path) {
        Ok(bytes) => Value::String(format!("sha256:{}", sha256_hex(&bytes))),
        Err(_) => Value::String(String::new()),
    }
}

fn resources_json(module_id: &ModuleId, module: &Module) -> Vec<Value> {
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
                json!({
                    "id": format!("{module_id}#resource:formula-{formula_index}"),
                    "module_id": module_id.to_string(),
                    "unit_id": unit.to_string(),
                    "kind": "formula",
                    "span": span_json(*span),
                    "content_digest": sha256_hex(latex.as_bytes()),
                    "language": null,
                    "display": display,
                })
            }
            Resource::Code {
                unit,
                language,
                source: _,
                content_hash,
                span,
            } => {
                code_index += 1;
                json!({
                    "id": format!("{module_id}#resource:code-{code_index}"),
                    "module_id": module_id.to_string(),
                    "unit_id": unit.to_string(),
                    "kind": "code",
                    "span": span_json(*span),
                    "content_digest": content_hash,
                    "language": language,
                })
            }
        })
        .collect()
}

fn diagnostics_json(project: &LoadedProject) -> Vec<Value> {
    project
        .diagnostics
        .iter()
        .map(|item| {
            let (line, column) = item
                .line_col()
                .map(|(line, column)| (Value::from(line), Value::from(column)))
                .unwrap_or((Value::Null, Value::Null));
            json!({
                "module": item.module_id.to_string(),
                "relative_path": item.module_id.relative_path,
                "span": item.diagnostic.span.map(span_json),
                "line": line,
                "column": column,
                "severity": if item.diagnostic.is_error() { "error" } else { "warning" },
                "code": item.diagnostic.code,
                "message": item.diagnostic.message,
                "hints": item.diagnostic.hints,
            })
        })
        .collect()
}

fn units_json(module: &Module) -> Vec<Value> {
    module
        .units
        .iter()
        .map(|unit| {
            let kind = match unit.kind {
                UnitKind::Root => "root",
                UnitKind::Namespace => "namespace",
                UnitKind::Function => "function",
                UnitKind::Section => "section",
            };
            json!({
                "id": unit.id.to_string(),
                "kind": kind,
                "name": unit.name,
                "level": unit.level,
                "span": span_json(unit.span),
                "function": unit.function.as_ref().map(ToString::to_string),
            })
        })
        .collect()
}

fn function_json(function: &Function) -> Value {
    json!({
        "id": function.id.to_string(),
        "unit_id": function.unit.to_string(),
        "local_name": function.local_name,
        "namespace": function.namespace,
        "span": span_json(function.span),
        "heading_span": span_json(function.heading_span),
        "signature": {
            "kind": match function.signature {
                SignatureKind::Explicit => "explicit",
                SignatureKind::Inferred => "inferred",
            },
            "params": function.params.iter().map(|param| json!({
                "name": param.name,
                "required": param.default.is_none(),
                "type": param.type_annotation,
                "default": param.default.as_ref().map(expr_json),
                "span": span_json(param.span),
            })).collect::<Vec<_>>(),
        },
        "effects": ["unknown"],
        "body": function.body.iter().map(stmt_json).collect::<Vec<_>>(),
    })
}

fn stmt_json(stmt: &Stmt) -> Value {
    match stmt {
        Stmt::Assign {
            name, value, span, ..
        } => json!({
            "kind": "assign",
            "target": name,
            "value": expr_json(value),
            "span": span_json(*span),
        }),
        Stmt::Eval { expr, span } => json!({
            "kind": "expression",
            "value": expr_json(expr),
            "span": span_json(*span),
        }),
        Stmt::Call { call, span } => json!({
            "kind": "call_site",
            "target": call_target_json(&call.target),
            "args": call.args.iter().map(arg_json).collect::<Vec<_>>(),
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

fn call_target_json(target: &CallTarget) -> Value {
    match target {
        CallTarget::Function { name } => json!({"kind":"function","name":name}),
        CallTarget::Qualified {
            module_alias,
            function,
        } => json!({"kind":"qualified","module_alias":module_alias,"function":function}),
        CallTarget::Resolved { module, function } => {
            json!({"kind":"resolved","module":module.to_string(),"function":function})
        }
    }
}

fn reference_edges(project: &LoadedProject, module_id: &marqdo_ir::ModuleId) -> Vec<Value> {
    let Some(file) = project.files.get(module_id) else {
        return Vec::new();
    };
    let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
    let mut edges = Vec::new();
    file.analysis.document.root.walk(&mut |node| {
        let marqdo_markdown::MdKind::Link { url, .. } = &node.kind else {
            return;
        };
        edges.push(resolve_reference(
            project,
            module_id,
            &file.analysis.units,
            parent,
            url,
            node.span,
        ));
    });
    edges.sort_by_key(|edge| {
        edge.get("span")
            .and_then(|span| span.get("start"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    });
    edges
}

fn resolve_reference(
    project: &LoadedProject,
    source_module: &marqdo_ir::ModuleId,
    units: &marqdo_frontend::UnitIndex,
    parent: &Path,
    url: &str,
    span: Span,
) -> Value {
    if let Some(fragment) = url.strip_prefix('#') {
        let unit = units.anchors.get(fragment).or_else(|| {
            units
                .units
                .iter()
                .find(|unit| unit.name == fragment)
                .map(|unit| &unit.id)
        });
        return json!({
            "source": source_module.to_string(),
            "url": url,
            "span": span_json(span),
            "state": if unit.is_some() { "resolved" } else { "unresolved" },
            "target_module": source_module.to_string(),
            "target_unit": unit.map(ToString::to_string),
        });
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return json!({
            "source": source_module.to_string(),
            "url": url,
            "span": span_json(span),
            "state": "external",
            "target_module": null,
            "target_unit": null,
        });
    }
    let (path_part, fragment) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (url, None),
    };
    let candidate = parent.join(path_part);
    let Some(target_module) = project.module_id_for_path(&candidate) else {
        return json!({
            "source": source_module.to_string(),
            "url": url,
            "span": span_json(span),
            "state": "unresolved",
            "target_module": null,
            "target_unit": null,
        });
    };
    let target_units = project
        .files
        .get(&target_module)
        .map(|file| &file.analysis.units);
    let resolved = match (fragment, target_units) {
        (None, _) => Some(None),
        (Some(fragment), Some(units)) => units
            .anchors
            .get(fragment)
            .or_else(|| {
                units
                    .units
                    .iter()
                    .find(|unit| unit.name == fragment)
                    .map(|unit| &unit.id)
            })
            .map(|unit| Some(unit.to_string())),
        (Some(_), None) => None,
    };
    let (state, target_unit) = match resolved {
        Some(unit) => ("resolved", unit),
        None => ("unresolved", None),
    };
    json!({
        "source": source_module.to_string(),
        "url": url,
        "span": span_json(span),
        "state": state,
        "target_module": target_module.to_string(),
        "target_unit": target_unit,
    })
}

fn calls_json(project: &LoadedProject, module: &Module, function: &Function) -> Vec<Value> {
    let mut calls = Vec::new();
    collect_calls(project, module, function, &function.body, &mut calls);
    calls.sort_by_key(|call| {
        call.get("span")
            .and_then(|span| span.get("start"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    });
    calls
}

fn collect_calls(
    project: &LoadedProject,
    module: &Module,
    function: &Function,
    body: &[Stmt],
    calls: &mut Vec<Value>,
) {
    for stmt in body {
        match stmt {
            Stmt::Assign { value, .. } | Stmt::Eval { expr: value, .. } => {
                expr_calls(project, module, function, value, calls)
            }
            Stmt::Return { value, .. } => expr_calls(project, module, function, value, calls),
            Stmt::Call { call, span } => calls.push(json!({
                "function": function.id.to_string(),
                "span": span_json(*span),
                "callee": call_target_json(&call.target),
                "state": "resolved",
            })),
            Stmt::Branch { arms, .. } => {
                for arm in arms {
                    expr_calls(project, module, function, &arm.condition, calls);
                    collect_calls(project, module, function, &arm.body, calls);
                }
            }
            Stmt::Loop {
                condition, body, ..
            } => {
                expr_calls(project, module, function, condition, calls);
                collect_calls(project, module, function, body, calls);
            }
        }
    }
}

fn expr_calls(
    project: &LoadedProject,
    module: &Module,
    function: &Function,
    expr: &Expr,
    calls: &mut Vec<Value>,
) {
    match expr {
        Expr::Call { callee, args, span } => {
            let (state, target) = resolve_callee(project, module, callee);
            calls.push(json!({
                "function": function.id.to_string(),
                "span": span_json(*span),
                "callee": callee_json(callee),
                "state": state,
                "target": target,
            }));
            for arg in args {
                expr_calls(project, module, function, &arg.value, calls);
            }
        }
        Expr::Unary { operand, .. } => expr_calls(project, module, function, operand, calls),
        Expr::Binary { left, right, .. } => {
            expr_calls(project, module, function, left, calls);
            expr_calls(project, module, function, right, calls);
        }
        Expr::List(items) => {
            for item in items {
                expr_calls(project, module, function, item, calls);
            }
        }
        Expr::Record(entries) => {
            for entry in entries {
                expr_calls(project, module, function, &entry.value, calls);
            }
        }
        Expr::Member { base, .. } => expr_calls(project, module, function, base, calls),
        Expr::Index { base, index, .. } => {
            expr_calls(project, module, function, base, calls);
            expr_calls(project, module, function, index, calls);
        }
        _ => {}
    }
}

fn resolve_callee(
    project: &LoadedProject,
    module: &Module,
    callee: &Expr,
) -> (&'static str, Value) {
    match callee {
        Expr::Ident(ident) => {
            if marqdo_ir::builtins::find(&ident.name).is_some() {
                ("builtin", json!(ident.name))
            } else if let Some(function) = module.function(&ident.name) {
                ("function", json!(function.id.to_string()))
            } else {
                ("unresolved", Value::Null)
            }
        }
        Expr::Member { base, name, .. } => {
            if let Expr::Ident(alias) = base.as_ref() {
                let qualified = format!("{}.{}", alias.name, name);
                if marqdo_ir::builtins::find(&qualified).is_some() {
                    return ("builtin", json!(qualified));
                }
                if let Some(target_module) = project.program.resolve_import(&module.id, &alias.name)
                {
                    if let Some(target) = project.program.module(target_module) {
                        if let Some(function) = target.function(name) {
                            return ("function", json!(function.id.to_string()));
                        }
                    }
                }
                if project.program.external_callable(&qualified).is_some() {
                    return ("plugin", json!(qualified));
                }
            }
            ("unresolved", Value::Null)
        }
        _ => ("unresolved", Value::Null),
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
