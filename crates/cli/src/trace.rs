//! Public Trace v2 writer (spec/trace-v2.md).
//!
//! Events come from the interpreter (semantic) and the capability host
//! (effects). No argument values, secrets or handles are recorded.

use std::path::Path;

use marqdo_host::{Host, SemanticEvent, StdHost, TraceEntry};
use serde_json::{json, Value};

use crate::loader::LoadedProject;

/// Schema name and version.
pub const SCHEMA: &str = "marqdo.trace";
pub const SCHEMA_VERSION: u32 = 2;

/// Write a trace atomically for one run.
pub fn write(
    project: &LoadedProject,
    host: &StdHost,
    entry: &str,
    status: &str,
    path: &Path,
    run_id: Option<&str>,
) -> Result<(), String> {
    // A combined `--event-stream` run shares the CLI-generated opaque run id
    // with Execution Events v1; Trace v2 keeps its schema and field types.
    let run_id = run_id.map(str::to_string).unwrap_or_else(|| {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        )
    });
    let analysis_digest = crate::analysis::digest(project);
    let root_id = project.identity.root_id();
    let state = host.trace_state();
    let mut events: Vec<Value> = host
        .semantic_events()
        .iter()
        .map(semantic_json)
        .chain(host.trace().iter().map(effect_json))
        .collect();
    events.sort_by_key(|event| event.get("seq").and_then(Value::as_u64).unwrap_or(u64::MAX));

    let mut truncation = state.truncation.clone();
    let max_bytes = state.budget.max_bytes;
    let bytes = loop {
        let mut all = events.clone();
        if let Some(truncation) = &truncation {
            let seq = all
                .last()
                .and_then(|event| event.get("seq"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + 1;
            all.push(json!({
                "seq": seq,
                "kind": "trace.truncated",
                "module_id": null,
                "span": null,
                "detail": truncation.reason,
            }));
        }
        let trace = json!({
            "schema": SCHEMA,
            "schema_version": SCHEMA_VERSION,
            "run": {"id": run_id, "entry": entry, "status": status},
            "source": {
                "root_id": root_id,
                "analysis_digest": format!("sha256:{analysis_digest}"),
            },
            "limits": {
                "max_events": state.budget.max_events,
                "max_bytes": state.budget.max_bytes,
            },
            "truncated": truncation,
            "events": all,
        });
        let bytes = serde_json::to_vec_pretty(&trace)
            .map_err(|error| format!("cannot encode trace: {error}"))?;
        if max_bytes == 0 || bytes.len() <= max_bytes || events.is_empty() {
            break bytes;
        }
        events.pop();
        if truncation.is_none() {
            truncation = Some(marqdo_host::TraceTruncation {
                reason: "byte_budget".to_string(),
                limit: max_bytes as u64,
                written: events.len() as u64,
            });
        }
    };
    atomic_write(path, &bytes)
}

fn semantic_json(event: &SemanticEvent) -> Value {
    json!({
        "seq": event.seq,
        "kind": event.kind,
        "function_id": event.function_id,
        "module_id": event.module_id,
        "span": event.span.map(|span| json!({"start": span.start, "end": span.end})),
        "detail": event.detail,
    })
}

fn effect_json(entry: &TraceEntry) -> Value {
    json!({
        "seq": entry.seq,
        "kind": "effect",
        "module_id": entry.module,
        "span": entry.span.map(|span| json!({"start": span.start, "end": span.end})),
        "detail": entry.result,
        "operation": entry.op,
        "duration_micros": entry.duration_micros,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
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
            .unwrap_or_else(|| "trace.json".to_string()),
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
