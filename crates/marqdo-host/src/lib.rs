//! CapabilityHost: capability checks, resource budgets, effect tracing and the
//! only bridge to math, foreign kernels, files and plugins.
//!
//! Blueprint §9 and §7.3: every side effect goes through this boundary, checks
//! happen at the point of execution, and host failures are structured errors.

pub mod budget;
pub mod capability;
pub mod context;
pub mod fs;
pub mod handles;
pub mod host;
pub mod kernels;
pub mod observe;
mod plugin_worker;
mod process;
mod provider_c;
mod provider_worker;
pub mod resource;
pub mod trace;

pub use budget::{Budget, DEFAULT_FUEL, DEFAULT_OUTPUT_BYTES, DEFAULT_STACK_LIMIT};
pub use capability::{
    Capabilities, CapabilityAudit, CapabilityBroker, CapabilityHandler, CapabilityRequest,
    FsReadHandler, CORE_CAPABILITIES, RESERVED_HANDLER_PREFIXES,
};
pub use context::ExecutionContext;
pub use fs::ProjectFs;
pub use handles::{contains_plugin_handle, PluginHandleTable};
pub use host::{arity, expect_text, Host, HostOp, StdHost};
pub use marqdo_sandbox::{PlatformCapabilities, SandboxMode, StrictIsolation};
pub use observe::{
    encode_value, estimate_event_bytes, FrameId, ObservationConfig, ObservationError,
    ObservationEvent, ObservationSink, ObservedValue, ParameterSource, RuntimeObservation,
    ValueState, DEFAULT_MAX_OBSERVATION_BYTES, DEFAULT_MAX_OBSERVATION_EVENTS,
    DEFAULT_MAX_VALUE_BYTES, MAX_STREAM_CHUNK_BYTES,
};
pub use resource::{ResourceProvider, ResourceRead, ResourceTable, SharedResourceProvider};
pub use trace::{EffectTrace, SemanticEvent, TraceBudget, TraceEntry, TraceState, TraceTruncation};
