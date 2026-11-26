//! Deliver the Host observation sequence to JSONL or atomic JSON snapshots.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use marqdo_host::{
    ObservationConfig, ObservationError, ObservationEvent, ObservationSink, ValueState,
    DEFAULT_MAX_OBSERVATION_BYTES, DEFAULT_MAX_OBSERVATION_EVENTS, DEFAULT_MAX_VALUE_BYTES,
};
use marqdo_host_contract::{FrameId, ParameterSource};
use marqdo_value::Value;
use serde_json::{json, Value as Json};

/// Public schema name.
pub const SCHEMA: &str = "marqdo.observation";
/// Public schema major version.
pub const SCHEMA_VERSION: u32 = 1;
/// Delivery format on stdout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Jsonl,
}

/// CLI observation options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationOptions {
    pub output: OutputFormat,
    pub file: Option<PathBuf>,
    /// Exact local-variable names selected with `--observe-variable`.
    pub variables: Vec<String>,
    /// Exact parameter names selected with `--observe-parameter`.
    pub parameters: Vec<String>,
    /// Second explicit consent for `Text` bytes.
    pub allow_text_values: bool,
    /// Maximum encoded source bytes for one value (`0` = unlimited).
    pub max_value_bytes: usize,
    /// Structural/value event budget (`0` = unlimited).
    pub max_events: u64,
    /// Approximate structural/value byte budget (`0` = unlimited).
    pub max_bytes: u64,
}

impl Default for ObservationOptions {
    fn default() -> Self {
        Self {
            output: OutputFormat::Text,
            file: None,
            variables: Vec::new(),
            parameters: Vec::new(),
            allow_text_values: false,
            max_value_bytes: DEFAULT_MAX_VALUE_BYTES,
            max_events: DEFAULT_MAX_OBSERVATION_EVENTS,
            max_bytes: DEFAULT_MAX_OBSERVATION_BYTES,
        }
    }
}

impl ObservationOptions {
    pub fn enabled(&self) -> bool {
        self.output != OutputFormat::Text || self.file.is_some()
    }

    pub fn has_capture_overrides(&self) -> bool {
        let defaults = Self::default();
        !self.variables.is_empty()
            || !self.parameters.is_empty()
            || self.allow_text_values
            || self.max_value_bytes != defaults.max_value_bytes
            || self.max_events != defaults.max_events
            || self.max_bytes != defaults.max_bytes
    }

    /// Typed host policy for this run.
    pub fn config(&self) -> ObservationConfig {
        ObservationConfig {
            structural: true,
            selected_variables: self.variables.iter().cloned().collect::<BTreeSet<_>>(),
            selected_parameters: self.parameters.iter().cloned().collect::<BTreeSet<_>>(),
            allow_text_values: self.allow_text_values,
            max_observation_events: self.max_events,
            max_observation_bytes: self.max_bytes,
            max_value_bytes: self.max_value_bytes,
        }
    }
}

/// One run and event sequence, shared with the host observation sink.
#[derive(Clone)]
pub struct ObservationSession {
    inner: Rc<RefCell<EventWriter>>,
    config: ObservationConfig,
}

impl ObservationSession {
    /// Set up all destinations and publish the first event.
    pub fn open(options: &ObservationOptions) -> Result<Self, String> {
        let run_id = new_run_id();
        let config = options.config();
        let opened = json!({
            "producer": {
                "name": "marqdo",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "observation": {
                "level": config.level(),
                "values": config.values_enabled(),
                "selected_variables": config.selected_variables.iter().cloned().collect::<Vec<_>>(),
                "selected_parameters": config.selected_parameters.iter().cloned().collect::<Vec<_>>(),
                "allow_text_values": config.allow_text_values,
            },
            "limits": {
                "max_observation_events": config.max_observation_events,
                "max_observation_bytes": config.max_observation_bytes,
                "max_value_bytes": config.max_value_bytes,
            },
        });
        let mut writer = EventWriter {
            output: options.output,
            snapshot: Snapshot::open(options)?,
            run_id,
            seq: 0,
            source: Json::Null,
            truncated: Json::Null,
            capture: opened["observation"].clone(),
            failed: false,
        };
        writer.emit("stream.opened", opened)?;
        Ok(Self {
            inner: Rc::new(RefCell::new(writer)),
            config,
        })
    }

    /// Typed policy (used to install the host sink).
    pub fn config(&self) -> ObservationConfig {
        self.config.clone()
    }

    /// Installable host sink handle.
    pub fn host_sink(&self) -> HostEventSink {
        HostEventSink {
            stream: self.clone(),
        }
    }

    /// Write one raw public event. Used for lifecycle and terminal events.
    pub fn emit(&self, kind: &str, payload: Json) -> Result<(), String> {
        self.inner.borrow_mut().emit(kind, payload)
    }

    /// Write `run.started`.
    pub fn started(
        &self,
        entry: &str,
        mode: &str,
        root_id: &str,
        analysis_digest: &str,
    ) -> Result<(), String> {
        self.emit(
            "run.started",
            json!({
                "entry": entry,
                "mode": mode,
                "source": {
                    "root_id": root_id,
                    "analysis_digest": analysis_digest,
                },
                "started_unix_ms": unix_millis(),
            }),
        )
    }

    /// Write the successful terminal event and its explicitly returned result.
    pub fn completed(&self, value: &Value) -> Result<(), String> {
        let transferable = !value.contains_memory()
            && !value.contains_resource()
            && !marqdo_host::contains_plugin_handle(value);
        let wire = transferable.then(|| marqdo_value::to_wire(value));
        self.emit(
            "run.completed",
            json!({"status": "ok", "value": wire, "value_type": value.type_name()}),
        )
    }

    /// Write the failed terminal event.
    pub fn failed(&self, error: Json) -> Result<(), String> {
        self.emit("run.failed", json!({"status": "error", "error": error}))
    }

    /// Deliver a snapshot without a terminal event when isolation interrupted a run.
    pub fn incomplete(&self) -> Result<(), String> {
        self.inner.borrow_mut().finish_snapshot("incomplete")
    }

    /// Write the pre-execution rejection terminal event.
    pub fn rejected(&self, phase: &str, code: &str, message: &str) -> Result<(), String> {
        self.emit(
            "run.rejected",
            json!({
                "status": "rejected",
                "phase": phase,
                "error": {"code": code, "message": message},
            }),
        )
    }

    /// Encode and write one typed host/runtime observation event.
    pub fn emit_observation(&self, event: ObservationEvent<'_>) -> Result<(), String> {
        let (kind, payload) = observation_payload(event);
        self.emit(kind, payload)
    }
}

/// `ObservationSink` adapter installed into `StdHost`.
pub struct HostEventSink {
    stream: ObservationSession,
}

impl ObservationSink for HostEventSink {
    fn emit(&mut self, event: ObservationEvent<'_>) -> Result<(), ObservationError> {
        self.stream
            .emit_observation(event)
            .map_err(ObservationError::new)
    }
}

struct Snapshot {
    staging: tempfile::NamedTempFile,
    path: Option<PathBuf>,
}

impl Snapshot {
    fn open(options: &ObservationOptions) -> Result<Option<Self>, String> {
        if options.output != OutputFormat::Json && options.file.is_none() {
            return Ok(None);
        }
        let staging = if let Some(path) = &options.file {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create observation directory: {error}"))?;
            tempfile::NamedTempFile::new_in(parent)
        } else {
            tempfile::NamedTempFile::new()
        }
        .map_err(|error| format!("cannot stage observation snapshot: {error}"))?;
        let mut snapshot = Self {
            staging,
            path: options.file.clone(),
        };
        snapshot.write(b"{\"events\":[")?;
        Ok(Some(snapshot))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.staging
            .write_all(bytes)
            .map_err(|error| format!("cannot write observation snapshot: {error}"))
    }

    fn finish(mut self, metadata: Json, stdout: bool) -> Result<(), String> {
        let suffix = serde_json::to_vec(&metadata)
            .map_err(|error| format!("cannot encode observation metadata: {error}"))?;
        self.write(b"],")?;
        self.write(&suffix[1..])?;
        self.write(b"\n")?;
        if let Some(path) = &self.path {
            self.staging
                .as_file()
                .sync_all()
                .map_err(|error| format!("cannot sync observation snapshot: {error}"))?;
            let mut file = self
                .staging
                .persist(path)
                .map_err(|error| format!("cannot publish observation snapshot: {error}"))?;
            if stdout {
                write_snapshot_stdout(&mut file)?;
            }
        } else {
            write_snapshot_stdout(self.staging.as_file_mut())?;
        }
        Ok(())
    }
}

fn write_snapshot_stdout(file: &mut std::fs::File) -> Result<(), String> {
    let mut out = io::stdout().lock();
    file.rewind()
        .and_then(|()| io::copy(file, &mut out))
        .and_then(|_| out.flush())
        .map_err(|error| format!("cannot write observation snapshot to stdout: {error}"))
}

struct EventWriter {
    output: OutputFormat,
    snapshot: Option<Snapshot>,
    run_id: String,
    seq: u64,
    source: Json,
    truncated: Json,
    capture: Json,
    failed: bool,
}

impl EventWriter {
    fn emit(&mut self, kind: &str, payload: Json) -> Result<(), String> {
        if self.failed {
            return Err("observation transport already failed".into());
        }
        let result = self.deliver(kind, payload);
        if result.is_err() {
            self.failed = true;
            self.snapshot.take();
        }
        result
    }

    fn deliver(&mut self, kind: &str, payload: Json) -> Result<(), String> {
        match kind {
            "run.started" => self.source = payload["source"].clone(),
            "observation.truncated" => self.truncated = payload.clone(),
            _ => {}
        }
        let event = json!({
            "schema": SCHEMA,
            "schema_version": SCHEMA_VERSION,
            "run_id": self.run_id,
            "seq": self.seq,
            "kind": kind,
            "payload": payload,
        });
        let mut bytes = serde_json::to_vec(&event)
            .map_err(|error| format!("cannot encode observation event: {error}"))?;
        if let Some(snapshot) = &mut self.snapshot {
            if self.seq != 0 {
                snapshot.write(b",")?;
            }
            snapshot.write(&bytes)?;
        }
        if matches!(kind, "run.completed" | "run.failed" | "run.rejected") {
            self.finish_snapshot(payload["status"].as_str().expect("terminal status"))?;
        }
        if self.output == OutputFormat::Jsonl {
            let mut out = io::stdout().lock();
            bytes.push(b'\n');
            out.write_all(&bytes)
                .and_then(|()| out.flush())
                .map_err(|error| format!("cannot write observation stream: {error}"))?;
        }
        self.seq += 1;
        Ok(())
    }

    fn finish_snapshot(&mut self, status: &str) -> Result<(), String> {
        let Some(snapshot) = self.snapshot.take() else {
            return Ok(());
        };
        let mut capture = self.capture.clone();
        capture["truncated"] = self.truncated.clone();
        snapshot.finish(
            json!({
                "schema": SCHEMA,
                "schema_version": SCHEMA_VERSION,
                "run": {"id": self.run_id, "status": status, "source": self.source},
                "capture": capture,
            }),
            self.output == OutputFormat::Json,
        )
    }
}

/// The strict supervisor forwards only a complete current snapshot.
pub fn is_complete_snapshot(bytes: &[u8], exit_code: i32) -> bool {
    let Ok(snapshot) = serde_json::from_slice::<Json>(bytes) else {
        return false;
    };
    let Some(events) = snapshot["events"].as_array() else {
        return false;
    };
    let Some(run_id) = snapshot["run"]["id"].as_str() else {
        return false;
    };
    let Some(last) = events.last() else {
        return false;
    };
    let status = match last["kind"].as_str() {
        Some("run.completed") if exit_code == 0 => "ok",
        Some("run.failed") if exit_code != 0 => "error",
        Some("run.rejected") if exit_code != 0 => "rejected",
        _ => return false,
    };
    snapshot["schema"] == SCHEMA
        && snapshot["schema_version"] == SCHEMA_VERSION
        && snapshot["run"]["status"] == status
        && snapshot["capture"].is_object()
        && last["payload"]["status"] == status
        && events[0]["kind"] == "stream.opened"
        && events.iter().enumerate().all(|(seq, event)| {
            event["schema"] == SCHEMA
                && event["schema_version"] == SCHEMA_VERSION
                && event["run_id"] == run_id
                && event["seq"] == seq
        })
}

fn observation_payload(event: ObservationEvent<'_>) -> (&'static str, Json) {
    match event {
        ObservationEvent::StdoutChunk { data } => (
            "stdout.chunk",
            json!({
                "encoding": "base64",
                "data_base64": base64_encode(data),
                "byte_count": data.len(),
            }),
        ),
        ObservationEvent::StderrChunk { data } => (
            "stderr.chunk",
            json!({
                "encoding": "base64",
                "data_base64": base64_encode(data),
                "byte_count": data.len(),
            }),
        ),
        ObservationEvent::CallEnter {
            frame_id,
            parent_frame_id,
            function_id,
            module_id,
            span,
        } => (
            "call.enter",
            json!({
                "frame_id": frame_id.as_wire(),
                "parent_frame_id": parent_frame_id.map(FrameId::as_wire),
                "function_id": function_id,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::CallExit {
            frame_id,
            status,
            module_id,
            span,
        } => (
            "call.exit",
            json!({
                "frame_id": frame_id.as_wire(),
                "status": status,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::Return {
            frame_id,
            value_type,
            module_id,
            span,
        } => (
            "return",
            json!({
                "frame_id": frame_id.as_wire(),
                "value_type": value_type,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::BranchEval {
            frame_id,
            result,
            module_id,
            span,
        } => (
            "branch.eval",
            json!({
                "frame_id": frame_id.as_wire(),
                "result": result,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::LoopIteration {
            frame_id,
            decision,
            module_id,
            span,
        } => (
            "loop.iteration",
            json!({
                "frame_id": frame_id.as_wire(),
                "decision": decision,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::LoopTransfer {
            frame_id,
            transfer,
            module_id,
            span,
        } => (
            transfer.event_kind(),
            json!({
                "frame_id": frame_id.as_wire(),
                "module_id": module_id,
                "span": span_json(span),
            }),
        ),
        ObservationEvent::Effect {
            frame_id,
            operation,
            result_code,
            module_id,
            span,
        } => (
            "effect",
            json!({
                "frame_id": frame_id.map(FrameId::as_wire),
                "operation": operation,
                "result_code": result_code,
                "module_id": module_id,
                "span": span.map(span_json),
            }),
        ),
        ObservationEvent::VariableAssigned {
            frame_id,
            name,
            value_type,
            module_id,
            span,
            value_state,
            value,
        } => (
            "variable.assigned",
            value_event_payload(
                frame_id,
                name,
                None,
                value_type,
                module_id,
                span,
                value_state,
                value,
            ),
        ),
        ObservationEvent::ParameterObserved {
            frame_id,
            name,
            source,
            value_type,
            module_id,
            span,
            value_state,
            value,
        } => (
            "parameter.observed",
            value_event_payload(
                frame_id,
                name,
                Some(source),
                value_type,
                module_id,
                span,
                value_state,
                value,
            ),
        ),
        ObservationEvent::ObservationTruncated {
            reason,
            limit,
            written,
            dropped,
        } => (
            "observation.truncated",
            json!({
                "reason": reason,
                "limit": limit,
                "written": written,
                "dropped": dropped,
            }),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn value_event_payload(
    frame_id: FrameId,
    name: &str,
    source: Option<ParameterSource>,
    value_type: &str,
    module_id: &str,
    span: Option<marqdo_span::Span>,
    value_state: ValueState,
    value: Option<marqdo_host::ObservedValue>,
) -> Json {
    let mut payload = json!({
        "frame_id": frame_id.as_wire(),
        "name": name,
        "value_type": value_type,
        "module_id": module_id,
        "span": span.map(span_json),
        "value_state": value_state.as_str(),
    });
    if let Some(source) = source {
        payload["source"] = Json::String(source.as_str().to_string());
    }
    if let Some(value) = value {
        payload["value"] = value.wire;
    }
    payload
}

fn span_json(span: marqdo_span::Span) -> Json {
    json!({"start": span.start, "end": span.end})
}

fn new_run_id() -> String {
    format!("{}-{}", std::process::id(), unix_nanos())
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Standard padded base64 (no external dependency; the protocol only needs
/// one canonical encoding).
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(
            base64_encode("开始文本\n".as_bytes()),
            "5byA5aeL5paH5pysCg=="
        );
    }

    #[test]
    fn no_unselected_value_is_encoded() {
        let options = ObservationOptions {
            variables: vec!["visible".to_string()],
            ..ObservationOptions::default()
        };
        let config = options.config();
        assert_eq!(config.level(), "values");
        assert!(config.selected_variables.contains("visible"));
        assert!(!config.allow_text_values);
        let empty = ObservationOptions::default().config();
        assert_eq!(empty.level(), "structural");
        assert!(!empty.values_enabled());
    }
}
