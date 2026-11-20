//! Effect tracing.

use std::time::Duration;

use marqdo_span::Span;
use serde::{Deserialize, Serialize};

/// One recorded host effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEntry {
    /// Global event sequence within the run.
    #[serde(default)]
    pub seq: u64,
    /// Operation name (`print`, `fs.read`, `foreign.call`, ...).
    pub op: String,
    /// Module that initiated it.
    pub module: String,
    /// Source span of the call site.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Wall-clock duration.
    pub duration_micros: u64,
    /// Result summary (`ok`, or a diagnostic code). Never contains secrets.
    pub result: String,
}

/// Collected effects for one run.
#[derive(Debug, Clone, Default)]
pub struct EffectTrace {
    entries: Vec<TraceEntry>,
    /// Whether tracing is enabled.
    pub enabled: bool,
}

impl EffectTrace {
    /// Enable tracing.
    pub fn enabled() -> Self {
        Self {
            entries: Vec::new(),
            enabled: true,
        }
    }

    /// Record an effect.
    pub fn record(
        &mut self,
        seq: u64,
        op: impl Into<String>,
        module: impl Into<String>,
        span: Option<Span>,
        duration: Duration,
        result: impl Into<String>,
    ) {
        if !self.enabled {
            return;
        }
        self.entries.push(TraceEntry {
            seq,
            op: op.into(),
            module: module.into(),
            span,
            duration_micros: duration.as_micros() as u64,
            result: result.into(),
        });
    }

    /// Recorded entries.
    pub fn entries(&self) -> &[TraceEntry] {
        &self.entries
    }
}

/// Default maximum number of trace events.
pub const DEFAULT_MAX_TRACE_EVENTS: usize = 100_000;
/// Default approximate maximum serialized trace bytes.
pub const DEFAULT_MAX_TRACE_BYTES: usize = 32 * 1024 * 1024;

/// Trace capacity budget. A zero value means unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceBudget {
    /// Maximum events (including the terminal truncation marker).
    pub max_events: usize,
    /// Approximate maximum serialized bytes.
    pub max_bytes: usize,
}

impl Default for TraceBudget {
    fn default() -> Self {
        Self {
            max_events: DEFAULT_MAX_TRACE_EVENTS,
            max_bytes: DEFAULT_MAX_TRACE_BYTES,
        }
    }
}

/// Why and where a trace stopped recording.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraceTruncation {
    /// `event_budget` or `byte_budget`.
    pub reason: String,
    /// The configured limit that was reached.
    pub limit: u64,
    /// Events written before truncation.
    pub written: u64,
}

/// Mutable trace capacity state for one run.
#[derive(Debug, Clone, Default)]
pub struct TraceState {
    /// Whether trace recording is enabled.
    pub enabled: bool,
    /// Configured budget.
    pub budget: TraceBudget,
    /// Events accepted so far.
    pub written: usize,
    /// Estimated accepted bytes.
    pub bytes: usize,
    /// Truncation marker, set once.
    pub truncation: Option<TraceTruncation>,
}

impl TraceState {
    /// Accept one event if it fits the budget.
    pub fn accept(&mut self, estimate: usize) -> bool {
        if !self.enabled || self.truncation.is_some() {
            return false;
        }
        if self.budget.max_events > 0 && self.written >= self.budget.max_events {
            self.truncation = Some(TraceTruncation {
                reason: "event_budget".to_string(),
                limit: self.budget.max_events as u64,
                written: self.written as u64,
            });
            return false;
        }
        if self.budget.max_bytes > 0 && self.bytes + estimate > self.budget.max_bytes {
            self.truncation = Some(TraceTruncation {
                reason: "byte_budget".to_string(),
                limit: self.budget.max_bytes as u64,
                written: self.written as u64,
            });
            return false;
        }
        self.written += 1;
        self.bytes += estimate;
        true
    }

    /// Estimate one semantic/effect event envelope.
    pub fn estimate(
        kind: &str,
        function_id: Option<&str>,
        module_id: Option<&str>,
        detail: Option<&str>,
        operation: Option<&str>,
    ) -> usize {
        96 + kind.len()
            + function_id.map(str::len).unwrap_or(0)
            + module_id.map(str::len).unwrap_or(0)
            + detail.map(str::len).unwrap_or(0)
            + operation.map(str::len).unwrap_or(0)
    }
}

/// One semantic event emitted by the interpreter (Trace v2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvent {
    /// Global event sequence within the run.
    pub seq: u64,
    /// Stable event kind (`call.enter`, `call.exit`, `branch.eval`, ...).
    pub kind: String,
    /// Canonical function id, when inside a call.
    pub function_id: Option<String>,
    /// Logical module owning `span`.
    pub module_id: Option<String>,
    /// Source span, when known.
    pub span: Option<Span>,
    /// Non-sensitive detail (`ok`, `true`, `int`, an error code).
    pub detail: Option<String>,
}
