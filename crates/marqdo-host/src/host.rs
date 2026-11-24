//! CapabilityHost: the only entry point for side effects.
//!
//! The runtime never touches files, processes or the math kernel directly; it
//! calls [`Host::invoke`]. Authorization is checked here, at the point of
//! execution, not at parse/compile time.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use marqdo_plugin_abi::{
    ErrorKind as PluginErrorKind, LoadedPlugin, PluginError, PluginKind, PluginManifest,
};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, ResourceHandle, RuntimeError, Value};
use rustls::pki_types::CertificateDer;

use crate::budget::Budget;
use crate::capability::{Capabilities, CapabilityAudit, CapabilityBroker};
use crate::context::ExecutionContext;
use crate::fs::ProjectFs;
use crate::handles::{contains_plugin_handle, PluginHandleTable};
use crate::observe::{
    encode_value, estimate_event_bytes, FrameId, ObservationConfig, ObservationError,
    ObservationEvent, ObservationSink, ObservationState, RuntimeObservation,
};
use crate::plugin_worker::WorkerPlugin;
use crate::provider_c::{load_package, CResourceProvider};
use crate::provider_worker::WorkerResourceProvider;
use crate::resource::{
    is_core_resource_kind, ResourceProvider, ResourceRead, ResourceTable, SharedResourceProvider,
};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode};

mod dispatch;
mod providers;
mod resource_ops;
mod resource_requests;

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

/// Default capability host implementation.
pub struct StdHost {
    /// Requested sandbox mode.
    pub mode: SandboxMode,
    /// Platform capability report.
    pub platform: PlatformCapabilities,
    /// Effective grants.
    pub capabilities: Capabilities,
    /// Bounded resources.
    pub budget: Budget,
    /// Filesystem view.
    pub fs: ProjectFs,
    /// Maximum wall-clock time for one foreign kernel call.
    pub foreign_timeout: Duration,
    /// Maximum captured bytes per foreign stream.
    pub foreign_output_limit: usize,
    /// Registered foreign kernels (built-in Python/Node plus explicit providers).
    pub kernel_registry: marqdo_foreign::KernelRegistry,
    output: Vec<u8>,
    trace_id: u64,
    /// Explicitly trusted, in-process native plugins.
    plugins: Vec<PluginSlot>,
    /// Complete export name -> index into `plugins` (ADR-0010).
    plugin_exports: BTreeMap<String, usize>,
    /// Trusted vendor capability policy entries for the broker.
    vendor_capabilities: BTreeSet<String>,
    /// Extra TLS roots explicitly trusted by the embedder (DER).
    tls_extra_roots: Vec<CertificateDer<'static>>,
    /// Shared audit log for plugin capability requests.
    broker_audit: Arc<Mutex<Vec<CapabilityAudit>>>,
    /// Host-issued opaque plugin handles.
    pub handles: PluginHandleTable,
    /// Program stderr captured separately from stdout and diagnostics.
    stderr: Vec<u8>,
    /// Host-owned resource/stream table (ADR-0025).
    resources: ResourceTable,
    /// Registered in-process providers keyed by resource kind (ADR-0033).
    resource_providers: BTreeMap<String, (String, SharedResourceProvider)>,
    observation: Option<ObservationState>,
    observation_error: Option<String>,
}

struct PluginSlot {
    /// Package name used for capability grants.
    package: String,
    /// Artifact/manifest digest that produced this registration.
    manifest_digest: String,
    /// Complete export name -> declared signature.
    exports: BTreeMap<String, String>,
    plugin: PluginTransport,
}

enum PluginTransport {
    Native(LoadedPlugin),
    Worker(WorkerPlugin),
}

impl PluginTransport {
    fn invoke(
        &mut self,
        request: &marqdo_plugin_abi::WireRequest,
    ) -> Result<marqdo_plugin_abi::WireResponse, PluginError> {
        match self {
            PluginTransport::Native(plugin) => plugin.invoke(request),
            PluginTransport::Worker(plugin) => plugin.invoke(request),
        }
    }
}

impl StdHost {
    /// Create a host with the given mode and effective capabilities.
    pub fn new(
        mode: SandboxMode,
        capabilities: Capabilities,
        project_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        let trace_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        Self {
            mode,
            platform: PlatformCapabilities::detect(),
            capabilities,
            budget: Budget::default(),
            fs: ProjectFs::new(project_root),
            foreign_timeout: Duration::from_secs(10),
            foreign_output_limit: 1024 * 1024,
            kernel_registry: marqdo_foreign::KernelRegistry::builtin(),
            output: Vec::new(),
            trace_id,
            plugins: Vec::new(),
            plugin_exports: BTreeMap::new(),
            vendor_capabilities: BTreeSet::new(),
            tls_extra_roots: Vec::new(),
            broker_audit: Arc::new(Mutex::new(Vec::new())),
            handles: PluginHandleTable::default(),
            stderr: Vec::new(),
            resources: ResourceTable::new(),
            resource_providers: BTreeMap::new(),
            observation: None,
            observation_error: None,
        }
    }

    /// Add one explicit TLS root certificate (DER) for `resource.open("tls")`.
    ///
    /// System roots are always tried first; this adds a private CA for
    /// embedders and tests. The certificate is a trust decision, not a grant:
    /// the exact host grant is still required.
    pub fn with_tls_ca(mut self, certificate: CertificateDer<'static>) -> Self {
        self.tls_extra_roots.push(certificate);
        self
    }

    /// Load a native plugin in-process after an explicit trust decision.
    ///
    /// In-process native code shares the host address space and is **not**
    /// sandboxed; this build has no isolated worker transport. The caller must
    /// pass `trusted = true` and grant the plugin capability. Every export is
    /// registered by its complete dotted name (ADR-0010); the whole load is
    /// rejected atomically if any name conflicts.
    pub fn load_plugin(
        &mut self,
        manifest: &PluginManifest,
        base_dir: &std::path::Path,
        trusted: bool,
    ) -> Result<Vec<String>, RuntimeError> {
        self.capabilities
            .check_plugin(&manifest.plugin.name, None)?;
        manifest.validate().map_err(map_plugin_error)?;
        let mut exports = BTreeMap::new();
        for export in &manifest.plugin.exports {
            if !export.name.contains('.') {
                return Err(RuntimeError::new(
                    ErrorCategory::PluginAbi,
                    None,
                    format!("plugin export `{}` must be namespaced", export.name),
                ));
            }
            if self.plugin_exports.contains_key(&export.name) {
                return Err(RuntimeError::new(
                    ErrorCategory::PluginFailure,
                    None,
                    format!(
                        "plugin export `{}` is already registered by another plugin",
                        export.name
                    ),
                ));
            }
            exports.insert(export.name.clone(), export.signature.clone());
        }
        let artifact = manifest
            .resolve_artifact(&marqdo_plugin_abi::HostTarget::current())
            .map_err(map_plugin_error)?;
        let manifest_digest = artifact.sha256.clone();
        let transport = match manifest.plugin.kind {
            PluginKind::Native => {
                if !trusted {
                    let mut error = RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        None,
                        "loading an in-process native plugin requires explicit trust",
                    );
                    error.effect_trace.push(
                        "hint: use `kind: worker` for crash isolation, or pass --trust-native-plugin for in-process native code"
                            .to_string(),
                    );
                    return Err(error);
                }
                let mut broker = CapabilityBroker::with_audit(
                    self.capabilities.clone(),
                    Arc::clone(&self.broker_audit),
                )
                .with_fs(self.fs.clone());
                for capability in &self.vendor_capabilities {
                    broker.allow_vendor(capability.clone());
                }
                let package = manifest.plugin.name.clone();
                let options = marqdo_plugin_abi::LoadOptions {
                    host_target: marqdo_plugin_abi::HostTarget::current(),
                    capability_handler: Some(Box::new(move |capability, request| {
                        broker.execute(&package, capability, request)
                    })),
                };
                PluginTransport::Native(
                    LoadedPlugin::open_with(manifest, base_dir, options)
                        .map_err(map_plugin_error)?,
                )
            }
            PluginKind::Worker => {
                let mut broker = CapabilityBroker::with_audit(
                    self.capabilities.clone(),
                    Arc::clone(&self.broker_audit),
                )
                .with_fs(self.fs.clone());
                for capability in &self.vendor_capabilities {
                    broker.allow_vendor(capability.clone());
                }
                let package = manifest.plugin.name.clone();
                let handler: marqdo_plugin_abi::CapabilityHandler =
                    Box::new(move |capability, request| {
                        broker.execute(&package, capability, request)
                    });
                PluginTransport::Worker(
                    WorkerPlugin::spawn(manifest, base_dir, Some(handler))
                        .map_err(map_plugin_error)?,
                )
            }
            PluginKind::Wasm => {
                return Err(RuntimeError::new(
                    ErrorCategory::KernelUnavailable,
                    None,
                    "plugin kind `wasm` is not supported in this build",
                ))
            }
        };
        let index = self.plugins.len();
        self.plugins.push(PluginSlot {
            package: manifest.plugin.name.clone(),
            manifest_digest,
            exports: exports.clone(),
            plugin: transport,
        });
        let names: Vec<String> = exports.keys().cloned().collect();
        for name in &names {
            self.plugin_exports.insert(name.clone(), index);
        }
        Ok(names)
    }

    /// Trust a vendor capability for future plugin loads.
    pub fn allow_vendor_capability(&mut self, capability: impl Into<String>) {
        self.vendor_capabilities.insert(capability.into());
    }

    /// Registered plugin exports as `(complete name, signature, digest, package)`.
    pub fn plugin_exports(&self) -> Vec<(String, String, String, String)> {
        let mut exports = Vec::new();
        for slot in &self.plugins {
            for (name, signature) in &slot.exports {
                exports.push((
                    name.clone(),
                    signature.clone(),
                    slot.manifest_digest.clone(),
                    slot.package.clone(),
                ));
            }
        }
        exports.sort();
        exports
    }

    /// Audited plugin capability decisions.
    pub fn capability_audit(&self) -> Vec<CapabilityAudit> {
        self.broker_audit
            .lock()
            .map(|audit| audit.clone())
            .unwrap_or_default()
    }

    /// Register an explicit external foreign-kernel provider.
    pub fn register_kernel(
        &mut self,
        descriptor: marqdo_foreign::KernelDescriptor,
    ) -> Result<(), RuntimeError> {
        self.kernel_registry.register(descriptor).map_err(|error| {
            RuntimeError::new(ErrorCategory::KernelUnavailable, None, error.to_string())
        })
    }

    /// Install the typed observation sink and policy (RFC-OBS-0001).
    ///
    /// Without this call the host has no observation state: it does not
    /// allocate events, clone values or serialize anything.
    pub fn with_observation(
        mut self,
        config: ObservationConfig,
        sink: Box<dyn ObservationSink>,
    ) -> Self {
        self.observation = Some(ObservationState::new(config, sink));
        self
    }

    /// The installed observation policy, when event mode is active.
    pub fn observation_config(&self) -> Option<&ObservationConfig> {
        self.observation.as_ref().map(|state| &state.config)
    }

    /// A saved observation transport failure (if any).
    ///
    /// The interpreter surfaces this at the next budget boundary; the CLI also
    /// checks it after the run so a successful return cannot mask a dead
    /// event pipe.
    pub fn observation_failure(&self) -> Option<&str> {
        self.observation_error.as_deref()
    }

    fn emit_observation(
        &mut self,
        event: ObservationEvent<'_>,
        critical: bool,
    ) -> Result<(), RuntimeError> {
        let Some(state) = self.observation.as_mut() else {
            return Ok(());
        };
        let mut pending = Some(event);
        if !critical {
            if state.is_truncated() {
                return Ok(());
            }
            let estimate = estimate_event_bytes(pending.as_ref().expect("event present"));
            if !state.admit(estimate) {
                let reason = state.truncation_reason(estimate);
                let limit = state.limit_for(reason);
                let written = state.written();
                pending = Some(ObservationEvent::ObservationTruncated {
                    reason,
                    limit,
                    written,
                    dropped: None,
                });
            }
        }
        let result = match pending {
            Some(event) => state.sink.emit(event),
            None => Ok(()),
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) => Err(self.record_observation_error(error)),
        }
    }

    fn record_observation_error(&mut self, error: ObservationError) -> RuntimeError {
        self.observation_error = Some(error.message.clone());
        error.into_runtime_error()
    }

    pub(crate) fn observe_effect(
        &mut self,
        operation: &str,
        result_code: &str,
        ctx: &ExecutionContext,
    ) {
        let frame_id = self
            .observation
            .as_ref()
            .and_then(ObservationState::parent_frame);
        let result = self.emit_observation(
            ObservationEvent::Effect {
                frame_id,
                operation,
                result_code,
                module_id: &ctx.module_id,
                span: ctx.span,
            },
            false,
        );
        // Effect recording is an existing infallible path. The failure is
        // stored and surfaced by the next fuel/deadline boundary and by the
        // CLI after the run; it is never silently forgotten.
        let _ = result;
    }

    /// Override the budget.
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }

    /// Current execution context for a call site.
    pub fn context(&self, module: impl Into<String>, span: Option<Span>) -> ExecutionContext {
        ExecutionContext::new(module, span).with_trace_id(self.trace_id)
    }

    /// Buffered program output without consuming it.
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Attach run-scoped stdin bytes at assembly time (composition root).
    ///
    /// This does not start a thread, spawn a process or read a hidden second
    /// channel. A program reads these bytes only through `stdio/stdin`.
    pub fn with_stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.resources.set_stdin(bytes.into());
        self
    }

    /// Buffered program stderr without consuming it.
    pub fn error(&self) -> &[u8] {
        &self.stderr
    }

    /// Take buffered program stderr.
    pub fn take_error(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stderr)
    }

    fn dispatch(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        dispatch::dispatch(self, op, args, ctx)
    }

    fn op_fs_read(
        &mut self,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let path = expect_text(&args, 0, "fs.read path", ctx.span)?;
        self.capabilities.check_read(&path, ctx.span)?;
        let contents = self.fs.read(&path, ctx.span)?;
        self.budget.consume_output(contents.len(), ctx.span)?;
        Ok(Value::Text(contents))
    }

    fn op_plugin_invoke(
        &mut self,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        if args.len() != 2 {
            return Err(arity(ctx.span, "plugin.invoke", 2, args.len()));
        }
        let Value::Text(export_name) = &args[0] else {
            return Err(RuntimeError::type_error(
                ctx.span,
                "plugin export name must be Text",
            ));
        };
        let Value::Map(named_args) = &args[1] else {
            return Err(RuntimeError::type_error(
                ctx.span,
                "plugin call arguments must be named",
            ));
        };
        let Some(index) = self.plugin_exports.get(export_name).copied() else {
            return Err(RuntimeError::new(
                ErrorCategory::UnknownSymbol,
                ctx.span,
                format!("no plugin export `{export_name}` is loaded"),
            ));
        };
        let package = self.plugins[index].package.clone();
        self.capabilities.check_plugin(&package, ctx.span)?;
        let mut request = marqdo_plugin_abi::WireRequest::new(export_name.clone());
        for (name, value) in named_args {
            request = request.with_arg(name.clone(), value);
        }
        let response = self.plugins[index]
            .plugin
            .invoke(&request)
            .map_err(map_plugin_error)?;
        let value = response.typed_value().map_err(map_plugin_error)?;
        if let Some(value) = &value {
            if contains_plugin_handle(value) {
                return Err(RuntimeError::new(
                    ErrorCategory::PluginFailure,
                    ctx.span,
                    "plugin_handle.untrusted: plugin output may not mint host handles",
                ));
            }
        }
        Ok(value.unwrap_or(Value::Unit))
    }

    fn op_fs_write(
        &mut self,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let path = expect_text(&args, 0, "fs.write path", ctx.span)?;
        let contents = expect_text(&args, 1, "fs.write contents", ctx.span)?;
        self.capabilities.check_write(&path, ctx.span)?;
        self.fs.write(&path, &contents, ctx.span)?;
        Ok(Value::Unit)
    }
}

impl Host for StdHost {
    fn invoke(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        dispatch::invoke(self, op, args, ctx)
    }

    fn resource_open(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        self.open_resource_stream(kind, descriptor, rights, ctx)
    }

    fn resource_read(
        &mut self,
        handle: ResourceHandle,
        max_bytes: usize,
        ctx: &ExecutionContext,
    ) -> Result<ResourceRead, RuntimeError> {
        let read = self.resources.read(handle, max_bytes, ctx.span)?;
        self.observe_effect("resource.read", "ok", ctx);
        Ok(read)
    }

    fn resource_write(
        &mut self,
        handle: ResourceHandle,
        bytes: &[u8],
        ctx: &ExecutionContext,
    ) -> Result<usize, RuntimeError> {
        let kind = self.resources.kind(handle, ctx.span)?;
        let stdio = self.resources.stdio_name(handle, ctx.span)?;
        let count = self.resources.write(handle, bytes, ctx.span)?;
        if kind == "stdio" {
            self.budget.consume_output(count, ctx.span)?;
            if stdio == Some("stderr") {
                self.stderr.extend_from_slice(&bytes[..count]);
            } else {
                self.output.extend_from_slice(&bytes[..count]);
            }
            // Real-time boundary: emit exactly the bytes the host just
            // committed, at this write, never by splitting the final buffer.
            let committed = &bytes[..count];
            for chunk in committed.chunks(crate::observe::MAX_STREAM_CHUNK_BYTES) {
                let event = if stdio == Some("stderr") {
                    ObservationEvent::StderrChunk { data: chunk }
                } else {
                    ObservationEvent::StdoutChunk { data: chunk }
                };
                self.emit_observation(event, true)?;
            }
        }
        self.observe_effect("resource.write", "ok", ctx);
        Ok(count)
    }

    fn resource_close(
        &mut self,
        handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        if let Some((relative, bytes)) = self.resources.pending_file_write(handle, ctx.span)? {
            self.fs.write_bytes(&relative, &bytes, ctx.span)?;
        }
        self.resources.close(handle, ctx.span)?;
        self.observe_effect("resource.close", "ok", ctx);
        Ok(())
    }

    fn resource_accept(
        &mut self,
        handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let accepted = self.resources.accept(handle, ctx.span)?;
        self.observe_effect("resource.accept", "ok", ctx);
        let mut map = BTreeMap::new();
        map.insert("ok".to_string(), Value::Bool(true));
        map.insert("value".to_string(), Value::Resource(accepted));
        Ok(Value::Map(map))
    }

    fn resource_request(
        &mut self,
        operation: &str,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        self.request_compatibility_operation(operation, args, ctx)
    }

    fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        if let Some(message) = &self.observation_error {
            return Err(RuntimeError::new(
                ErrorCategory::Observation,
                span,
                format!("observe.transport_failed: {message}"),
            ));
        }
        self.budget.consume_fuel(amount, span)
    }

    fn check_stack(&self, depth: usize, span: Option<Span>) -> Result<(), RuntimeError> {
        self.budget.check_stack(depth, span)
    }

    fn check_deadline(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        if let Some(message) = &self.observation_error {
            return Err(RuntimeError::new(
                ErrorCategory::Observation,
                span,
                format!("observe.transport_failed: {message}"),
            ));
        }
        self.budget.check_deadline(span)
    }

    fn observation_enabled(&self) -> bool {
        self.observation.is_some()
    }

    fn observation_begin_frame(
        &mut self,
        function_id: &str,
        module_id: &str,
        span: Option<Span>,
    ) -> Result<Option<FrameId>, RuntimeError> {
        let Some(state) = self.observation.as_mut() else {
            return Ok(None);
        };
        let frame_id = state.begin_frame();
        let parent_frame_id = state.parent_frame();
        state.push_frame(frame_id);
        self.emit_observation(
            ObservationEvent::CallEnter {
                frame_id,
                parent_frame_id,
                function_id,
                module_id,
                span,
            },
            false,
        )?;
        Ok(Some(frame_id))
    }

    fn observation_end_frame(
        &mut self,
        frame_id: Option<FrameId>,
        status: &str,
        module_id: &str,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let Some(frame_id) = frame_id else {
            return Ok(());
        };
        if let Some(state) = self.observation.as_mut() {
            state.pop_frame(frame_id);
        }
        self.emit_observation(
            ObservationEvent::CallExit {
                frame_id,
                status,
                module_id,
                span,
            },
            false,
        )
    }

    fn observe_runtime(&mut self, event: RuntimeObservation<'_>) -> Result<(), RuntimeError> {
        if self.observation.is_none() {
            return Ok(());
        }
        match event {
            RuntimeObservation::Effect {
                operation,
                module_id,
                span,
            } => {
                self.observe_effect(operation, "ok", &ExecutionContext::new(module_id, span));
                Ok(())
            }
            RuntimeObservation::Return {
                frame_id,
                value_type,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                self.emit_observation(
                    ObservationEvent::Return {
                        frame_id,
                        value_type,
                        module_id,
                        span,
                    },
                    false,
                )
            }
            RuntimeObservation::BranchEval {
                frame_id,
                result,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                self.emit_observation(
                    ObservationEvent::BranchEval {
                        frame_id,
                        result,
                        module_id,
                        span,
                    },
                    false,
                )
            }
            RuntimeObservation::LoopIteration {
                frame_id,
                decision,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                self.emit_observation(
                    ObservationEvent::LoopIteration {
                        frame_id,
                        decision,
                        module_id,
                        span,
                    },
                    false,
                )
            }
            RuntimeObservation::LoopTransfer {
                frame_id,
                transfer,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                self.emit_observation(
                    ObservationEvent::LoopTransfer {
                        frame_id,
                        transfer,
                        module_id,
                        span,
                    },
                    false,
                )
            }
            RuntimeObservation::VariableAssigned {
                frame_id,
                name,
                value,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                let Some(config) = self.observation.as_ref().map(|state| &state.config) else {
                    return Ok(());
                };
                let selected = config.selected_variables.contains(name);
                let (value_state, observed) = encode_value(config, selected, value);
                self.emit_observation(
                    ObservationEvent::VariableAssigned {
                        frame_id,
                        name,
                        value_type: value.type_name(),
                        module_id,
                        span,
                        value_state,
                        value: observed,
                    },
                    false,
                )
            }
            RuntimeObservation::ParameterBound {
                frame_id,
                name,
                source,
                value,
                module_id,
                span,
            } => {
                let Some(frame_id) = frame_id else {
                    return Ok(());
                };
                let Some(config) = self.observation.as_ref().map(|state| &state.config) else {
                    return Ok(());
                };
                let selected = config.selected_parameters.contains(name);
                let (value_state, observed) = encode_value(config, selected, value);
                self.emit_observation(
                    ObservationEvent::ParameterObserved {
                        frame_id,
                        name,
                        source,
                        value_type: value.type_name(),
                        module_id,
                        span,
                        value_state,
                        value: observed,
                    },
                    false,
                )
            }
        }
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

fn map_plugin_error(error: PluginError) -> RuntimeError {
    let category = match error.kind {
        PluginErrorKind::Abi => ErrorCategory::PluginAbi,
        PluginErrorKind::Capability => ErrorCategory::PermissionDenied,
        PluginErrorKind::Wire => ErrorCategory::Type,
        PluginErrorKind::Unsupported => ErrorCategory::KernelUnavailable,
        _ => ErrorCategory::PluginFailure,
    };
    RuntimeError::new(category, None, format!("{}: {}", error.code, error.message))
}

/// Type-checked argument helper.
pub fn expect_text(
    args: &[Value],
    index: usize,
    what: &str,
    span: Option<Span>,
) -> Result<String, RuntimeError> {
    match args.get(index) {
        Some(Value::Text(text)) => Ok(text.clone()),
        Some(other) => Err(RuntimeError::type_error(
            span,
            format!("{what} must be Text, got {}", other.type_name()),
        )),
        None => Err(arity(span, what, index + 1, args.len())),
    }
}

/// Arity error helper.
pub fn arity(span: Option<Span>, name: &str, expected: usize, actual: usize) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::Arity,
        span,
        format!("`{name}` expects {expected} argument(s), got {actual}"),
    )
}
