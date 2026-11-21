//! Trace/effect bookkeeping helpers for `StdHost`.
//!
//! Child module of `host`; it may use the parent's private trace state.

use super::*;

impl StdHost {
    pub(crate) fn next_seq(&mut self) -> u64 {
        let seq = self.next_event_seq;
        self.next_event_seq += 1;
        seq
    }

    pub(crate) fn record_resource_effect(
        &mut self,
        op: &str,
        ctx: &ExecutionContext,
        started: Instant,
        summary: &str,
    ) {
        let estimate = TraceState::estimate(
            "effect",
            None,
            Some(&ctx.module_id),
            Some(summary),
            Some(op),
        );
        if !self.trace_state.accept(estimate) {
            return;
        }
        let seq = self.next_seq();
        self.trace.record(
            seq,
            op,
            ctx.module_id.clone(),
            ctx.span,
            started.elapsed(),
            summary.to_string(),
        );
    }
}
