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

/// One semantic event emitted by the interpreter (Trace v1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvent {
    /// Global event sequence within the run.
    pub seq: u64,
    /// Stable event kind (`call.enter`, `call.exit`, `branch.eval`, ...).
    pub kind: String,
    /// Canonical function id, when inside a call.
    pub function_id: Option<String>,
    /// Source span, when known.
    pub span: Option<Span>,
    /// Non-sensitive detail (`ok`, `true`, `int`, an error code).
    pub detail: Option<String>,
}
