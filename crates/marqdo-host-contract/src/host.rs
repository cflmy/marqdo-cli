//! The host boundary used by the interpreter and future frontends.
//!
//! The trait is implemented by the native CLI host, by the independent
//! embedding/test host and by any future host. Implementations own effects,
//! grants and budgets; the runtime only requests operations through this
//! interface and never performs external I/O itself.

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, ResourceHandle, RuntimeError, Value};

use crate::context::ExecutionContext;
use crate::observe::{FrameId, RuntimeObservation};
use crate::resource::ResourceRead;

/// Host operations reachable from the language surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOp {
    /// One-shot foreign execution.
    ForeignRun,
    /// Typed foreign call.
    ForeignCall,
    /// Read a project file.
    FsRead,
    /// Write a project file.
    FsWrite,
    /// Invoke a plugin export.
    PluginInvoke,
}

impl HostOp {
    /// Stable operation name used in traces and diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            HostOp::ForeignRun => "foreign.run",
            HostOp::ForeignCall => "foreign.call",
            HostOp::FsRead => "fs.read",
            HostOp::FsWrite => "fs.write",
            HostOp::PluginInvoke => "plugin.invoke",
        }
    }
}

/// The host boundary used by the interpreter and future View/IDE frontends.
pub trait Host {
    /// Invoke one operation with already-evaluated arguments.
    fn invoke(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError>;

    /// Open a resource/stream by kind, descriptor and requested rights.
    ///
    /// Default: unavailable. Only the standard host implements the first
    /// resource kinds (ADR-0025).
    fn resource_open(
        &mut self,
        _kind: &str,
        _descriptor: &str,
        _rights: &[String],
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide resource streams",
        ))
    }

    /// Read up to `max_bytes` from a resource into host memory.
    fn resource_read(
        &mut self,
        _handle: ResourceHandle,
        _max_bytes: usize,
        ctx: &ExecutionContext,
    ) -> Result<ResourceRead, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide resource streams",
        ))
    }

    /// Write bytes to a resource; returns the accepted count.
    fn resource_write(
        &mut self,
        _handle: ResourceHandle,
        _bytes: &[u8],
        ctx: &ExecutionContext,
    ) -> Result<usize, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide resource streams",
        ))
    }

    /// Close a resource handle.
    fn resource_close(
        &mut self,
        _handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide resource streams",
        ))
    }

    /// Accept one inbound TCP connection from a `tcp-listener` handle.
    ///
    /// Default: unavailable. The standard host returns a new `Resource` in a
    /// recoverable `{ok,value}` map or `{ok:false,code:"tcp.timeout"}`.
    fn resource_accept(
        &mut self,
        _handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide tcp listeners",
        ))
    }

    /// Execute one non-streaming resource operation (ADR-0041).
    ///
    /// Default: unavailable. The standard host implements the `fs.*` group;
    /// every call is capability-checked and project-mode only.
    fn resource_request(
        &mut self,
        _operation: &str,
        _args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            "this host does not provide resource operations",
        ))
    }

    /// Consume evaluation fuel.
    fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError>;

    /// Check call-stack depth.
    fn check_stack(&self, depth: usize, span: Option<Span>) -> Result<(), RuntimeError>;

    /// Check the wall-clock deadline.
    fn check_deadline(&self, span: Option<Span>) -> Result<(), RuntimeError>;

    /// Take the buffered program output.
    fn take_output(&mut self) -> Vec<u8>;

    /// Whether the host has an installed observation sink.
    ///
    /// The default host is inert; the interpreter uses this to skip building
    /// any observation event when there is no capture destination.
    fn observation_enabled(&self) -> bool {
        false
    }

    /// Announce a successfully bound call frame.
    ///
    /// Returns the run-unique frame identity, or `None` when observation is
    /// disabled. Argument binding must have completed before this is called;
    /// a binding failure never produces a frame.
    fn observation_begin_frame(
        &mut self,
        _function_id: &str,
        _module_id: &str,
        _span: Option<Span>,
    ) -> Result<Option<FrameId>, RuntimeError> {
        Ok(None)
    }

    /// Close a previously announced frame, including failure paths.
    fn observation_end_frame(
        &mut self,
        _frame_id: Option<FrameId>,
        _status: &str,
        _module_id: &str,
        _span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Emit one borrowed runtime observation event. Default: ignored.
    fn observe_runtime(&mut self, _event: RuntimeObservation<'_>) -> Result<(), RuntimeError> {
        Ok(())
    }
}
