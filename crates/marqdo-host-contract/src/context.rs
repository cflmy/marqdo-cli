//! Per-call execution context passed to the host boundary.

use marqdo_span::Span;

/// Context attached to every host invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    /// Canonical module that initiated the operation.
    pub module_id: String,
    /// Source span of the call site, when known.
    pub span: Option<Span>,
    /// Correlates trace entries belonging to one run.
    pub trace_id: u64,
}

impl ExecutionContext {
    /// Create a context.
    pub fn new(module_id: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            module_id: module_id.into(),
            span,
            trace_id: 0,
        }
    }

    /// Set the trace id.
    pub fn with_trace_id(mut self, trace_id: u64) -> Self {
        self.trace_id = trace_id;
        self
    }
}
