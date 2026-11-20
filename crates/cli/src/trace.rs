//! Public Trace v1 writer (spec/trace-v1.md).
//!
//! Events come from the interpreter (semantic) and the capability host
//! (effects). No argument values, secrets or handles are recorded.

use std::path::Path;

use marqdo_host::{Host, SemanticEvent, StdHost, TraceEntry};
use serde_json::{json, Value};

use crate::loader::LoadedProject;

/// Schema name and version.
pub const SCHEMA: &str = "marqdo.trace";
pub const SCHEMA_VERSION: u32 = 1;

/// Write a trace atomically for one run.
pub fn write(
    project: &LoadedProject,
    host: &StdHost,
    entry: &str,
    status: &str,
    path: &Path,
) -> Result<(), String> {
    let run_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    let analysis_digest = crate::analysis::digest(project);
    let root_id = project.identity.root_id();
    let mut events: Vec<Value> = host
        .semantic_events()
        .iter()
        .map(semantic_json)
        .chain(host.trace().iter().map(effect_json))
        .collect();
    events.sort_by_key(|event| event.get("seq").and_then(Value::as_u64).unwrap_or(u64::MAX));
    let trace = json!({
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "run": {"id": run_id, "entry": entry, "status": status},
        "source": {"root_id": root_id, "analysis_digest": format!("sha256:{analysis_digest}")},
        "events": events,
    });
    let bytes = serde_json::to_vec_pretty(&trace)
        .map_err(|error| format!("cannot encode trace: {error}"))?;
    atomic_write(path, &bytes)
}

fn semantic_json(event: &SemanticEvent) -> Value {
    json!({
        "seq": event.seq,
        "kind": event.kind,
        "function_id": event.function_id,
        "span": event.span.map(|span| json!({"start": span.start, "end": span.end})),
        "detail": event.detail,
    })
}

fn effect_json(entry: &TraceEntry) -> Value {
    json!({
        "seq": entry.seq,
        "kind": "effect",
        "module": entry.module,
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
