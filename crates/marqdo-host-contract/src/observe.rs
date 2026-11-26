//! Runtime observation facts shared by the interpreter and the host contract.
//!
//! Only the types the interpreter itself produces and hands to [`crate::Host`]
//! live here. The host implementation keeps the concrete sink, redaction,
//! encoding and budget state; the public Observation protocol remains the
//! CLI's single producer. This module must not grow a second event vocabulary.

use marqdo_span::Span;
use marqdo_value::Value;

/// Opaque run-scoped call-instance identity.
///
/// Not a function identity, not a source location and never a memory pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(pub u64);

impl FrameId {
    /// Stable wire form used by the public protocol (`f1`, `f2`, ...).
    pub fn as_wire(self) -> String {
        format!("f{}", self.0)
    }
}

/// Where a bound parameter's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterSource {
    /// Supplied by the caller (CLI `--arg` or a Marqdo call).
    Provided,
    /// Evaluated from the declared default expression.
    Default,
}

impl ParameterSource {
    /// Stable wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ParameterSource::Provided => "provided",
            ParameterSource::Default => "default",
        }
    }
}

/// The action carried by a structured loop transfer observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopTransfer {
    /// Exit the enclosing loop.
    Break,
    /// Proceed to the enclosing loop's next condition check.
    Continue,
}

impl LoopTransfer {
    /// The event kind in Observation.
    pub fn event_kind(self) -> &'static str {
        match self {
            Self::Break => "control.break",
            Self::Continue => "control.continue",
        }
    }
}

/// Borrowed runtime notification accepted by [`crate::Host::observe_runtime`].
///
/// Value and control observations carry the owning call frame.
#[derive(Debug)]
pub enum RuntimeObservation<'a> {
    /// A local memory operation committed.
    Effect {
        operation: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A `return` statement committed (before the call exits).
    Return {
        frame_id: Option<FrameId>,
        value_type: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A branch condition evaluated.
    BranchEval {
        frame_id: Option<FrameId>,
        result: bool,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A loop condition evaluated.
    LoopIteration {
        frame_id: Option<FrameId>,
        decision: &'a str,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// An explicit loop control statement executed.
    LoopTransfer {
        frame_id: Option<FrameId>,
        transfer: LoopTransfer,
        module_id: &'a str,
        span: Span,
    },
    /// A local variable assignment committed.
    VariableAssigned {
        frame_id: Option<FrameId>,
        name: &'a str,
        value: &'a Value,
        module_id: &'a str,
        span: Option<Span>,
    },
    /// A parameter was bound.
    ParameterBound {
        frame_id: Option<FrameId>,
        name: &'a str,
        source: ParameterSource,
        value: &'a Value,
        module_id: &'a str,
        span: Option<Span>,
    },
}
