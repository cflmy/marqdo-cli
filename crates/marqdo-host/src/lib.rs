//! Native Host: the one concrete implementation of the neutral
//! [`marqdo_host_contract::Host`] boundary.
//!
//! Every side effect goes through this crate, checks happen at the point of
//! execution, and host failures are structured errors. The crate owns the OS
//! adapters, capability grants, resource handles, observation sink and trusted
//! extension execution; it never owns language semantics and never depends back
//! on the runtime, frontend or IR.
//!
//! Responsibility map (ADR-0071):
//! - [`capability`]/[`budget`]: grants, broker, audit, fuel/stack/deadline/output;
//! - [`resource`], [`fs`], `process`: resource kinds, handles and OS effects;
//! - [`plugin`], `plugin_worker`, `provider_c`, `provider_worker`, [`kernels`]:
//!   trusted plugin/provider/foreign execution (no second static binder);
//! - [`observe`]: the runtime observation sink, redaction, budget and encoding;
//! - [`host`]: `StdHost` assembly and the single dispatch table.

pub mod budget;
pub mod capability;
pub mod fs;
pub mod handles;
pub mod host;
pub mod kernels;
pub mod observe;
pub mod plugin;
mod plugin_worker;
mod process;
mod provider_c;
mod provider_worker;
pub mod resource;

pub use budget::{Budget, DEFAULT_FUEL, DEFAULT_OUTPUT_BYTES, DEFAULT_STACK_LIMIT};
pub use capability::{
    Capabilities, CapabilityAudit, CapabilityBroker, CapabilityHandler, CapabilityRequest,
    FsReadHandler, CORE_CAPABILITIES, RESERVED_HANDLER_PREFIXES,
};
pub use fs::ProjectFs;
pub use handles::{contains_plugin_handle, PluginHandleTable};
pub use host::{arity, expect_text, StdHost};
pub use marqdo_sandbox::{PlatformCapabilities, SandboxMode, StrictIsolation};
pub use observe::{
    encode_value, estimate_event_bytes, ObservationConfig, ObservationError, ObservationEvent,
    ObservationSink, ObservedValue, ValueState, DEFAULT_MAX_OBSERVATION_BYTES,
    DEFAULT_MAX_OBSERVATION_EVENTS, DEFAULT_MAX_VALUE_BYTES, MAX_STREAM_CHUNK_BYTES,
};
pub use resource::{
    is_core_resource_kind, ResourceProvider, ResourceTable, SharedResourceProvider,
    CORE_RESOURCE_KINDS,
};
