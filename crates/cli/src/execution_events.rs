//! Execution Events v1 JSONL writer and public envelope.
//!
//! Normative contract: `spec/execution-events-v1.md`; decision record:
//! ADR-0059. The host owns the typed events; this module only assigns the
//! public envelope/sequence, encodes bytes as base64 and writes one flushed
//! line per event to the CLI protocol stdout.
//!
//! There is no HTTP, SSE, WebSocket, View or Agent branch here.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use marqdo_host::{
    FrameId, ObservationConfig, ObservationError, ObservationEvent, ObservationSink,
    ParameterSource, ValueState, DEFAULT_MAX_OBSERVATION_BYTES, DEFAULT_MAX_OBSERVATION_EVENTS,
    DEFAULT_MAX_VALUE_BYTES,
};
use marqdo_value::{RuntimeError, Value};
use serde_json::{json, Value as Json};

/// Public schema name.
pub const SCHEMA: &str = "marqdo.execution-events";
/// Public schema major version.
pub const SCHEMA_VERSION: u32 = 1;
/// The only accepted `--event-stream` format in v1.
pub const FORMAT_JSONL: &str = "jsonl";

/// CLI observation options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationOptions {
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

/// One run's JSONL writer, shared with the host sink.
#[derive(Clone)]
pub struct EventStream {
    inner: Rc<RefCell<JsonlWriter>>,
    config: ObservationConfig,
}

impl EventStream {
    /// Open the protocol channel and write `stream.opened` as `seq: 0`.
    pub fn open(options: &ObservationOptions) -> Result<Self, String> {
        let run_id = new_run_id();
        let config = options.config();
        let opened = json!({
            "producer": {
                "name": "marqdo",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "protocol": {
                "name": SCHEMA,
                "schema_version": SCHEMA_VERSION,
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
        let mut writer = JsonlWriter {
            out: Box::new(io::stdout()),
            run_id: run_id.clone(),
            seq: 0,
        };
        writer.emit("stream.opened", opened)?;
        Ok(Self {
            inner: Rc::new(RefCell::new(writer)),
            config,
        })
    }

    /// Opaque run identity shared with Trace v2 when both are requested.
    pub fn run_id(&self) -> String {
        self.inner.borrow().run_id.clone()
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

    /// Write the successful terminal event.
    pub fn completed(&self, value: &Value) -> Result<(), String> {
        self.emit(
            "run.completed",
            json!({"status": "ok", "value": marqdo_value::to_wire(value)}),
        )
    }

    /// Write the failed terminal event.
    pub fn failed(&self, error: &RuntimeError) -> Result<(), String> {
        self.emit(
            "run.failed",
            json!({
                "status": "error",
                "error": {
                    "code": error.code(),
                    "message": error.message,
                    "module_id": error.module_id.as_deref(),
                    "span": error.span.map(span_json),
                }
            }),
        )
    }

    /// Write a failed terminal event for a CLI-level failure that still has a
    /// stable code (for example `cli.trace_failed`).
    pub fn failed_error(&self, code: &str, message: &str) -> Result<(), String> {
        self.emit(
            "run.failed",
            json!({
                "status": "error",
                "error": {"code": code, "message": message},
            }),
        )
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
    stream: EventStream,
}

impl ObservationSink for HostEventSink {
    fn emit(&mut self, event: ObservationEvent<'_>) -> Result<(), ObservationError> {
        self.stream
            .emit_observation(event)
            .map_err(ObservationError::new)
    }
}

struct JsonlWriter {
    out: Box<dyn Write>,
    run_id: String,
    seq: u64,
}

impl JsonlWriter {
    fn emit(&mut self, kind: &str, payload: Json) -> Result<(), String> {
        let event = json!({
            "schema": SCHEMA,
            "schema_version": SCHEMA_VERSION,
            "run_id": self.run_id,
            "seq": self.seq,
            "kind": kind,
            "payload": payload,
        });
        let mut line = serde_json::to_vec(&event)
            .map_err(|error| format!("cannot encode execution event: {error}"))?;
        line.push(b'\n');
        // Flush each line: line-buffered batching would hide real-time output
        // behind the run duration (RFC-OBS-0001 §5.2).
        self.out
            .write_all(&line)
            .and_then(|()| self.out.flush())
            .map_err(|error| format!("cannot write execution event stream: {error}"))?;
        self.seq += 1;
        Ok(())
    }
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
            operation,
            result_code,
            module_id,
            span,
        } => (
            "effect",
            json!({
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
