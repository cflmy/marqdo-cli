//! Typed runtime observation facade (RFC-OBS-0001, ADR-0059).
//!
//! This module is internal to the workspace. It defines the typed events the
//! interpreter and host emit at real commit points, the optional sink the CLI
//! installs, the run-scoped frame identity and the bounded observation budget.
//! It does **not** know JSONL, HTTP, SSE, View, agents or the public envelope;
//! the CLI composes the public Observation protocol from these events.
//!
//! Defaults are deliberately inert: no sink means no allocation and no
//! serialization. The facade never exposes `Frame.locals`, the whole
//! `MemoryStore`, raw OS pointers or host handles.

use std::collections::BTreeSet;

use marqdo_host_contract::{FrameId, LoopTransfer, ParameterSource};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};

/// Default maximum number of structural/value observation events per run.
pub const DEFAULT_MAX_OBSERVATION_EVENTS: u64 = 100_000;
/// Default approximate maximum encoded bytes for structural/value events.
pub const DEFAULT_MAX_OBSERVATION_BYTES: u64 = 16 * 1024 * 1024;
/// Default maximum encoded bytes for one selected value.
pub const DEFAULT_MAX_VALUE_BYTES: usize = 256;
/// Maximum committed stdio bytes carried by one chunk event.
pub const MAX_STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// Observation policy for one run.
///
/// `structural` always implies frame identity; value events only exist for
/// exact names in `selected_variables`/`selected_parameters`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationConfig {
    /// Publish lifecycle/frame/branch/loop/effect events.
    pub structural: bool,
    /// Exact local-variable names allowed to publish a value.
    pub selected_variables: BTreeSet<String>,
    /// Exact parameter names allowed to publish a value.
    pub selected_parameters: BTreeSet<String>,
    /// Second explicit consent required to publish `Text` bytes.
    pub allow_text_values: bool,
    /// Structural/value event budget (`0` = unlimited).
    pub max_observation_events: u64,
    /// Approximate structural/value byte budget (`0` = unlimited).
    pub max_observation_bytes: u64,
    /// Maximum source bytes encoded for one value (`0` = unlimited).
    pub max_value_bytes: usize,
}

impl Default for ObservationConfig {
    fn default() -> Self {
        Self {
            structural: true,
            selected_variables: BTreeSet::new(),
            selected_parameters: BTreeSet::new(),
            allow_text_values: false,
            max_observation_events: DEFAULT_MAX_OBSERVATION_EVENTS,
            max_observation_bytes: DEFAULT_MAX_OBSERVATION_BYTES,
            max_value_bytes: DEFAULT_MAX_VALUE_BYTES,
        }
    }
}

impl ObservationConfig {
    /// Whether any value can ever be published.
    pub fn values_enabled(&self) -> bool {
        !self.selected_variables.is_empty() || !self.selected_parameters.is_empty()
    }

    /// Negotiated observation level (`base`, `structural` or `values`).
    pub fn level(&self) -> &'static str {
        if self.values_enabled() {
            "values"
        } else if self.structural {
            "structural"
        } else {
            "base"
        }
    }
}

/// Why a value has (or has not) a wire encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueState {
    /// Name not selected; no value encoded.
    NotRequested,
    /// Name selected but policy hides the value.
    Redacted,
    /// Encoded but truncated to the configured maximum.
    Truncated,
    /// Type not supported by the v1 scalar policy.
    Unsupported,
    /// Encoded normally.
    Observed,
}

impl ValueState {
    /// Stable wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ValueState::NotRequested => "not_requested",
            ValueState::Redacted => "redacted",
            ValueState::Truncated => "truncated",
            ValueState::Unsupported => "unsupported",
            ValueState::Observed => "observed",
        }
    }
}

/// A bounded, policy-filtered value encoding (the `marqdo.value.v1` wire
/// object plus optional truncation metadata).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedValue {
    /// JSON object safe to publish.
    pub wire: serde_json::Value,
}

/// One typed internal observation event.
///
/// Borrows strings and stdio bytes so the default disabled path never clones a
/// frame, argument list or whole `Value`. The sink must consume it
/// synchronously before returning.
#[derive(Debug, Clone, PartialEq)]
pub enum ObservationEvent<'a> {
    /// Bytes committed to the program's stdout by the host.
    StdoutChunk { data: &'a [u8] },
    /// Bytes committed to the program's stderr by the host.
    StderrChunk { data: &'a [u8] },
    /// A new frame exists (arguments already bound).
    CallEnter {
        frame_id: FrameId,
        parent_frame_id: Option<FrameId>,
        function_id: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A frame finished, including failure paths.
    CallExit {
        frame_id: FrameId,
        status: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A `return` statement committed its value.
    Return {
        frame_id: FrameId,
        value_type: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A branch condition was evaluated.
    BranchEval {
        frame_id: FrameId,
        result: bool,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A loop condition was evaluated.
    LoopIteration {
        frame_id: FrameId,
        decision: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// An explicit loop control statement executed.
    LoopTransfer {
        frame_id: FrameId,
        transfer: LoopTransfer,
        module_id: &'a str,
        span: Span,
    },
    /// The host completed a controlled effect attempt.
    Effect {
        frame_id: Option<FrameId>,
        operation: &'a str,
        result_code: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A local assignment was committed.
    VariableAssigned {
        frame_id: FrameId,
        name: &'a str,
        value_type: &'a str,
        module_id: &'a str,
        span: Option<Span>,
        value_state: ValueState,
        value: Option<ObservedValue>,
    },
    /// A function parameter was bound.
    ParameterObserved {
        frame_id: FrameId,
        name: &'a str,
        source: ParameterSource,
        value_type: &'a str,
        module_id: &'a str,
        span: Option<Span>,
        value_state: ValueState,
        value: Option<ObservedValue>,
    },
    /// The structural/value observation budget was exhausted.
    ObservationTruncated {
        reason: &'a str,
        limit: u64,
        written: u64,
        dropped: Option<u64>,
    },
}

/// Sink installed by the composition root (the CLI JSONL writer in product
/// builds, an in-memory recorder in tests).
pub trait ObservationSink {
    /// Consume one event synchronously.
    fn emit(&mut self, event: ObservationEvent<'_>) -> Result<(), ObservationError>;
}

/// A sink failed to accept an event (usually a closed/broken output pipe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationError {
    /// Human-readable transport reason (safe to print to OS stderr).
    pub message: String,
}

impl ObservationError {
    /// Construct an observation transport error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Convert into the stable runtime failure used to abort the run.
    pub fn into_runtime_error(self) -> RuntimeError {
        RuntimeError::new(
            ErrorCategory::Observation,
            None,
            format!("observe.transport_failed: {}", self.message),
        )
    }
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ObservationError {}

/// Mutable per-run observation state owned by `StdHost`.
pub(crate) struct ObservationState {
    pub(crate) config: ObservationConfig,
    pub(crate) sink: Box<dyn ObservationSink>,
    next_frame: u64,
    frames: Vec<FrameId>,
    written: u64,
    bytes: u64,
    truncated: bool,
}

impl ObservationState {
    pub(crate) fn new(config: ObservationConfig, sink: Box<dyn ObservationSink>) -> Self {
        Self {
            config,
            sink,
            next_frame: 0,
            frames: Vec::new(),
            written: 0,
            bytes: 0,
            truncated: false,
        }
    }

    pub(crate) fn begin_frame(&mut self) -> FrameId {
        self.next_frame += 1;
        FrameId(self.next_frame)
    }

    pub(crate) fn parent_frame(&self) -> Option<FrameId> {
        self.frames.last().copied()
    }

    pub(crate) fn push_frame(&mut self, frame: FrameId) {
        self.frames.push(frame);
    }

    /// Pop the frame if it is on the stack. A mismatch is a runtime bug, not a
    /// consumer-visible event, so it is repaired defensively rather than
    /// panicking inside the host.
    pub(crate) fn pop_frame(&mut self, frame: FrameId) {
        if self.frames.last().copied() == Some(frame) {
            self.frames.pop();
        } else if let Some(position) = self.frames.iter().rposition(|item| *item == frame) {
            self.frames.truncate(position);
        }
    }

    /// Estimate one encoded event and accept it if the budget allows.
    ///
    /// Returns `Ok(true)` when the event was admitted, `Ok(false)` when the
    /// budget was already exhausted, and writes the single truncation marker
    /// through `emit_truncated` on the transition.
    pub(crate) fn admit(&mut self, estimate: u64) -> bool {
        if self.truncated {
            return false;
        }
        let event_limit = self.config.max_observation_events;
        let byte_limit = self.config.max_observation_bytes;
        if (event_limit > 0 && self.written >= event_limit)
            || (byte_limit > 0 && self.bytes.saturating_add(estimate) > byte_limit)
        {
            self.truncated = true;
            return false;
        }
        self.written += 1;
        self.bytes = self.bytes.saturating_add(estimate);
        true
    }

    pub(crate) fn truncation_reason(&self, estimate: u64) -> &'static str {
        let event_limit = self.config.max_observation_events;
        if event_limit > 0 && self.written >= event_limit {
            "event_budget"
        } else if self.config.max_observation_bytes > 0
            && self.bytes.saturating_add(estimate) > self.config.max_observation_bytes
        {
            "byte_budget"
        } else {
            "event_budget"
        }
    }

    pub(crate) fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub(crate) fn written(&self) -> u64 {
        self.written
    }

    pub(crate) fn limit_for(&self, reason: &str) -> u64 {
        if reason == "byte_budget" {
            self.config.max_observation_bytes
        } else {
            self.config.max_observation_events
        }
    }
}

/// Encode a selected value under the v1 scalar policy.
///
/// No `Debug`, no recursive container expansion and no handle serialization.
pub fn encode_value(
    config: &ObservationConfig,
    selected: bool,
    value: &Value,
) -> (ValueState, Option<ObservedValue>) {
    if !selected {
        return (ValueState::NotRequested, None);
    }
    match value {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => (
            ValueState::Observed,
            Some(ObservedValue {
                wire: marqdo_value::to_wire(value),
            }),
        ),
        Value::Text(text) => {
            if !config.allow_text_values {
                return (ValueState::Redacted, None);
            }
            let limit = config.max_value_bytes;
            if limit == 0 || text.len() <= limit {
                return (
                    ValueState::Observed,
                    Some(ObservedValue {
                        wire: marqdo_value::to_wire(value),
                    }),
                );
            }
            let mut cut = limit.min(text.len());
            while cut > 0 && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            let prefix = Value::Text(text[..cut].to_string());
            let mut wire = marqdo_value::to_wire(&prefix);
            if let serde_json::Value::Object(map) = &mut wire {
                map.insert("truncated".to_string(), serde_json::Value::Bool(true));
                map.insert(
                    "original_byte_count".to_string(),
                    serde_json::Value::from(text.len() as u64),
                );
            }
            (ValueState::Truncated, Some(ObservedValue { wire }))
        }
        _ => (ValueState::Unsupported, None),
    }
}

/// Approximate encoded size, used only for the observation byte budget.
pub fn estimate_event_bytes(event: &ObservationEvent<'_>) -> u64 {
    let mut total: u64 = 160;
    let mut add_text = |text: &str| total = total.saturating_add(text.len() as u64);
    match event {
        ObservationEvent::StdoutChunk { data } | ObservationEvent::StderrChunk { data } => {
            total = total.saturating_add((data.len() as u64).saturating_mul(4) / 3);
        }
        ObservationEvent::CallEnter {
            function_id,
            module_id,
            ..
        } => {
            add_text(function_id);
            add_text(module_id);
        }
        ObservationEvent::CallExit {
            status, module_id, ..
        } => {
            add_text(status);
            add_text(module_id);
        }
        ObservationEvent::Return {
            value_type,
            module_id,
            ..
        } => {
            add_text(value_type);
            add_text(module_id);
        }
        ObservationEvent::BranchEval { module_id, .. }
        | ObservationEvent::LoopIteration { module_id, .. }
        | ObservationEvent::LoopTransfer { module_id, .. }
        | ObservationEvent::Effect { module_id, .. } => {
            add_text(module_id);
            if let ObservationEvent::LoopIteration { decision, .. } = event {
                add_text(decision);
            }
            if let ObservationEvent::Effect { operation, .. } = event {
                add_text(operation);
            }
        }
        ObservationEvent::VariableAssigned {
            name,
            value_type,
            module_id,
            value,
            ..
        }
        | ObservationEvent::ParameterObserved {
            name,
            value_type,
            module_id,
            value,
            ..
        } => {
            add_text(name);
            add_text(value_type);
            add_text(module_id);
            if let Some(value) = value {
                total = total.saturating_add(encoded_len(&value.wire));
            }
        }
        ObservationEvent::ObservationTruncated { reason, .. } => add_text(reason),
    }
    total
}

fn encoded_len(value: &serde_json::Value) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(0)
}
