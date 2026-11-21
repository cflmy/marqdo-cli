//! CapabilityHost: the only entry point for side effects.
//!
//! The runtime never touches files, processes or the math kernel directly; it
//! calls [`Host::invoke`]. Authorization is checked here, at the point of
//! execution, not at parse/compile time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
use crate::plugin_worker::WorkerPlugin;
use crate::provider_c::{load_package, CResourceProvider};
use crate::provider_worker::WorkerResourceProvider;
use crate::resource::{ResourceProvider, ResourceRead, ResourceTable, SharedResourceProvider};
use crate::trace::{EffectTrace, SemanticEvent, TraceBudget, TraceEntry, TraceState};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode};

mod dispatch;
mod effects;
mod providers;
mod resource_ops;

/// Host operations reachable from the language surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOp {
    /// Parse a formula.
    MathParse,
    /// Evaluate a formula.
    MathEval,
    /// Substitute variables in a formula.
    MathSubstitute,
    /// Simplify a formula.
    MathSimplify,
    /// Differentiate a formula.
    MathDifferentiate,
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
            HostOp::MathParse => "math.parse",
            HostOp::MathEval => "math.eval",
            HostOp::MathSubstitute => "math.substitute",
            HostOp::MathSimplify => "math.simplify",
            HostOp::MathDifferentiate => "math.differentiate",
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

    /// Recorded effects.
    fn trace(&self) -> &[TraceEntry];

    /// Record one semantic event (Trace v1). Default: ignored.
    fn record_semantic(
        &mut self,
        _kind: &str,
        _function_id: Option<&str>,
        _module_id: Option<&str>,
        _span: Option<Span>,
        _detail: Option<&str>,
    ) {
    }

    /// Recorded semantic events. Default: empty.
    fn semantic_events(&self) -> &[SemanticEvent] {
        &[]
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
    /// Effect trace.
    pub trace: EffectTrace,
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
    semantic_events: Vec<SemanticEvent>,
    next_event_seq: u64,
    trace_state: TraceState,
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
            trace: EffectTrace::default(),
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
            semantic_events: Vec::new(),
            next_event_seq: 0,
            trace_state: TraceState::default(),
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

    /// Recorded semantic events (Trace v1).
    pub fn semantic_events(&self) -> &[SemanticEvent] {
        &self.semantic_events
    }

    /// Trace capacity state for the writer.
    pub fn trace_state(&self) -> &TraceState {
        &self.trace_state
    }

    /// Override the trace event/byte budget.
    pub fn with_trace_budget(mut self, budget: TraceBudget) -> Self {
        self.trace_state.budget = budget;
        self
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

    /// Enable effect tracing.
    pub fn with_trace(mut self) -> Self {
        self.trace = EffectTrace::enabled();
        self.trace_state.enabled = true;
        self
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

fn result_error(code: &str, message: &str) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    map.insert("code".to_string(), Value::text(code));
    map.insert("message".to_string(), Value::text(message));
    Value::Map(map)
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
        let started = Instant::now();
        let kind = self.resources.kind(handle, ctx.span)?;
        let read = self.resources.read(handle, max_bytes, ctx.span)?;
        self.record_semantic(
            "resource.read",
            None,
            Some(&ctx.module_id),
            ctx.span,
            Some(&format!(
                "kind={kind} count={} eof={}",
                read.bytes.len(),
                read.eof
            )),
        );
        self.record_resource_effect(
            "resource.read",
            ctx,
            started,
            &format!("kind={kind} count={}", read.bytes.len()),
        );
        Ok(read)
    }

    fn resource_write(
        &mut self,
        handle: ResourceHandle,
        bytes: &[u8],
        ctx: &ExecutionContext,
    ) -> Result<usize, RuntimeError> {
        let started = Instant::now();
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
        }
        self.record_semantic(
            "resource.write",
            None,
            Some(&ctx.module_id),
            ctx.span,
            Some(&format!("kind={kind} count={count}")),
        );
        self.record_resource_effect(
            "resource.write",
            ctx,
            started,
            &format!("kind={kind} count={count}"),
        );
        Ok(count)
    }

    fn resource_close(
        &mut self,
        handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        let started = Instant::now();
        let pending = self.resources.pending_file_write(handle, ctx.span)?;
        let count = match pending {
            Some((relative, bytes)) => {
                self.fs.write_bytes(&relative, &bytes, ctx.span)?;
                bytes.len()
            }
            None => 0,
        };
        self.resources.close(handle, ctx.span)?;
        self.record_semantic(
            "resource.close",
            None,
            Some(&ctx.module_id),
            ctx.span,
            Some(&format!("flushed_bytes={count}")),
        );
        self.record_resource_effect(
            "resource.close",
            ctx,
            started,
            &format!("flushed_bytes={count}"),
        );
        Ok(())
    }

    fn resource_accept(
        &mut self,
        handle: ResourceHandle,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let started = Instant::now();
        let accepted = self.resources.accept(handle, ctx.span)?;
        self.record_semantic(
            "resource.accept",
            None,
            Some(&ctx.module_id),
            ctx.span,
            Some("kind=tcp-listener"),
        );
        self.record_resource_effect("resource.accept", ctx, started, "ok");
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
        let started = Instant::now();
        if self.mode != SandboxMode::Project {
            return Err(RuntimeError::new(
                ErrorCategory::PermissionDenied,
                ctx.span,
                "resource operations are available only in project mode",
            ));
        }
        if args.iter().any(Value::contains_memory) || args.iter().any(Value::contains_resource) {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                ctx.span,
                "resource operations accept only Text path arguments",
            ));
        }
        if operation == "process.run" {
            if args.len() != 2 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `process.run` expects 2 arguments, got {}",
                        args.len()
                    ),
                ));
            }
            let program_type = args[0].type_name();
            let Value::Text(program) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!(
                        "resource operation `process.run` expects a Text program, got {program_type}"
                    ),
                ));
            };
            let args_type = args[1].type_name();
            let Value::List(values) = args[1].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!(
                        "resource operation `process.run` expects a List of Text arguments, got {args_type}"
                    ),
                ));
            };
            let mut program_args = Vec::with_capacity(values.len());
            for value in values {
                let value_type = value.type_name();
                let Value::Text(value) = value else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("process arguments must be Text, got {value_type}"),
                    ));
                };
                program_args.push(value);
            }
            self.capabilities.check_process(&program, ctx.span)?;
            let result = crate::process::run_process(self.fs.root(), &program, &program_args, ctx)?;
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=process.run"),
            );
            self.record_resource_effect("resource.request", ctx, started, "process.run");
            return Ok(result);
        }
        if operation == "compress.gzip" {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `compress.gzip` expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            let text_type = args[0].type_name();
            let Value::Text(text) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("compress.gzip expects Text, got {text_type}"),
                ));
            };
            if text.len() > 8 * 1024 * 1024 {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceExhausted,
                    ctx.span,
                    "compress.gzip input exceeds 8 MiB",
                ));
            }
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(text.as_bytes()).map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::HostError,
                    ctx.span,
                    format!("gzip failed: {error}"),
                )
            })?;
            let bytes = encoder.finish().map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::HostError,
                    ctx.span,
                    format!("gzip failed: {error}"),
                )
            })?;
            let values = bytes
                .into_iter()
                .map(|byte| Value::int(byte as i64))
                .collect();
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=compress.gzip"),
            );
            self.record_resource_effect("resource.request", ctx, started, "compress.gzip");
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("value".to_string(), Value::List(values));
            return Ok(Value::Map(map));
        }
        if operation == "compress.gunzip" {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `compress.gunzip` expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            let list_type = args[0].type_name();
            let Value::List(values) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("compress.gunzip expects List<Int>, got {list_type}"),
                ));
            };
            if values.len() > 8 * 1024 * 1024 {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceExhausted,
                    ctx.span,
                    "compress.gunzip input exceeds 8 MiB",
                ));
            }
            let mut bytes = Vec::with_capacity(values.len());
            for value in values {
                let Value::Int(value) = value else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        "compress.gunzip bytes must be Int",
                    ));
                };
                let Some(byte) = value.to_string().parse::<u8>().ok() else {
                    return Err(RuntimeError::new(
                        ErrorCategory::Type,
                        ctx.span,
                        format!("`{value}` is not a byte in 0..=255"),
                    ));
                };
                bytes.push(byte);
            }
            let mut decoder = flate2::read::GzDecoder::new(bytes.as_slice());
            let mut output = Vec::new();
            if let Err(error) = decoder
                .by_ref()
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut output)
            {
                return Ok(result_error(
                    "compress.invalid",
                    &format!("gunzip failed: {error}"),
                ));
            }
            if output.len() > 8 * 1024 * 1024 {
                return Ok(result_error(
                    "compress.too_large",
                    "gunzip output exceeds 8 MiB",
                ));
            }
            let text = match String::from_utf8(output) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(result_error(
                        "compress.not_utf8",
                        "gunzip output is not UTF-8",
                    ))
                }
            };
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=compress.gunzip"),
            );
            self.record_resource_effect("resource.request", ctx, started, "compress.gunzip");
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("value".to_string(), Value::text(text));
            return Ok(Value::Map(map));
        }
        if operation == "random.bytes" {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `random.bytes` expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            let count = match &args[0] {
                Value::Int(value) => value
                    .to_string()
                    .parse::<usize>()
                    .ok()
                    .filter(|count| (1..=65536).contains(count)),
                _ => None,
            };
            let Some(count) = count else {
                return Err(RuntimeError::new(
                    ErrorCategory::Type,
                    ctx.span,
                    "random.bytes requires an Int in 1..=65536",
                ));
            };
            self.capabilities.check_random(ctx.span)?;
            let mut bytes = vec![0u8; count];
            getrandom::fill(&mut bytes).map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::HostError,
                    ctx.span,
                    format!("OS entropy is unavailable: {error}"),
                )
            })?;
            let values = bytes
                .into_iter()
                .map(|byte| Value::int(byte as i64))
                .collect();
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=random.bytes"),
            );
            self.record_resource_effect("resource.request", ctx, started, "random.bytes");
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("value".to_string(), Value::List(values));
            return Ok(Value::Map(map));
        }
        if operation == "env.get" {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `env.get` expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            let value_type = args[0].type_name();
            let Value::Text(name) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("resource operation `env.get` expects a Text name, got {value_type}"),
                ));
            };
            self.capabilities.check_env(&name, ctx.span)?;
            let mut map = BTreeMap::new();
            match std::env::var(&name) {
                Ok(value) => {
                    map.insert("ok".to_string(), Value::Bool(true));
                    map.insert("value".to_string(), Value::text(value));
                }
                Err(std::env::VarError::NotPresent) => {
                    map.insert("ok".to_string(), Value::Bool(false));
                    map.insert("code".to_string(), Value::text("env.not_found"));
                    map.insert(
                        "message".to_string(),
                        Value::text(format!("environment variable `{name}` is not set")),
                    );
                }
                Err(std::env::VarError::NotUnicode(_)) => {
                    map.insert("ok".to_string(), Value::Bool(false));
                    map.insert("code".to_string(), Value::text("env.not_unicode"));
                    map.insert(
                        "message".to_string(),
                        Value::text(format!("environment variable `{name}` is not valid UTF-8")),
                    );
                }
            }
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=env.get"),
            );
            self.record_resource_effect("resource.request", ctx, started, "env.get");
            return Ok(Value::Map(map));
        }
        if operation == "clock.now" {
            if !args.is_empty() {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `clock.now` expects 0 arguments, got {}",
                        args.len()
                    ),
                ));
            }
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("system clock is before the Unix epoch: {error}"),
                    )
                })?
                .as_millis()
                .min(i64::MAX as u128) as i64;
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("value".to_string(), Value::int(millis));
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=clock.now"),
            );
            self.record_resource_effect("resource.request", ctx, started, "clock.now");
            return Ok(Value::Map(map));
        }
        let Some(name) = operation.strip_prefix("fs.") else {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                ctx.span,
                format!("unsupported resource operation `{operation}`"),
            ));
        };
        if name == "read_at" {
            if args.len() != 3 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `fs.read_at` expects 3 arguments, got {}",
                        args.len()
                    ),
                ));
            }
            let path_type = args[0].type_name();
            let Value::Text(path) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("fs.read_at path must be Text, got {path_type}"),
                ));
            };
            let offset = match &args[1] {
                Value::Int(value) => value.to_string(),
                other => {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.read_at offset must be Int, got {}", other.type_name()),
                    ))
                }
            };
            let length = match &args[2] {
                Value::Int(value) => value.to_string(),
                other => {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.read_at length must be Int, got {}", other.type_name()),
                    ))
                }
            };
            self.capabilities.check_read(&path, ctx.span)?;
            let result = self
                .fs
                .request("read_at", &[path, offset, length], ctx.span)?;
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=fs.read_at"),
            );
            self.record_resource_effect("resource.request", ctx, started, "fs.read_at");
            return Ok(result);
        }
        if name == "append" {
            if args.len() != 2 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `fs.append` expects 2 arguments, got {}",
                        args.len()
                    ),
                ));
            }
            let path_type = args[0].type_name();
            let Value::Text(path) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("fs.append path must be Text, got {path_type}"),
                ));
            };
            let text_type = args[1].type_name();
            let Value::Text(text) = args[1].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("fs.append text must be Text, got {text_type}"),
                ));
            };
            self.capabilities.check_write(&path, ctx.span)?;
            let result = self.fs.request("append", &[path, text], ctx.span)?;
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=fs.append"),
            );
            self.record_resource_effect("resource.request", ctx, started, "fs.append");
            return Ok(result);
        }
        if name == "walk" {
            if args.len() != 1 {
                return Err(RuntimeError::new(
                    ErrorCategory::Arity,
                    ctx.span,
                    format!(
                        "resource operation `fs.walk` expects 1 argument, got {}",
                        args.len()
                    ),
                ));
            }
            let path_type = args[0].type_name();
            let Value::Text(path) = args[0].clone() else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!("fs.walk path must be Text, got {path_type}"),
                ));
            };
            self.capabilities.check_read(&path, ctx.span)?;
            let result = self.fs.request("walk", &[path], ctx.span)?;
            self.record_semantic(
                "resource.request",
                None,
                Some(&ctx.module_id),
                ctx.span,
                Some("op=fs.walk"),
            );
            self.record_resource_effect("resource.request", ctx, started, "fs.walk");
            return Ok(result);
        }
        let expected = match name {
            "exists" | "metadata" | "list" | "mkdir" | "remove" => 1,
            "rename" => 2,
            other => {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    ctx.span,
                    format!("unsupported filesystem operation `{other}`"),
                ))
            }
        };
        if args.len() != expected {
            return Err(RuntimeError::new(
                ErrorCategory::Arity,
                ctx.span,
                format!(
                    "resource operation `{operation}` expects {expected} argument(s), got {}",
                    args.len()
                ),
            ));
        }
        let mut paths = Vec::with_capacity(args.len());
        for value in args {
            let value_type = value.type_name();
            let Value::Text(path) = value else {
                return Err(RuntimeError::type_error(
                    ctx.span,
                    format!(
                        "resource operation `{operation}` expects Text paths, got {value_type}"
                    ),
                ));
            };
            paths.push(path);
        }
        match name {
            "exists" | "metadata" | "list" => {
                self.capabilities.check_read(&paths[0], ctx.span)?;
            }
            "mkdir" | "remove" => {
                self.capabilities.check_write(&paths[0], ctx.span)?;
            }
            "rename" => {
                self.capabilities.check_write(&paths[0], ctx.span)?;
                self.capabilities.check_write(&paths[1], ctx.span)?;
            }
            _ => unreachable!(),
        }
        let result = self.fs.request(name, &paths, ctx.span)?;
        self.record_semantic(
            "resource.request",
            None,
            Some(&ctx.module_id),
            ctx.span,
            Some(&format!("op=fs.{name}")),
        );
        self.record_resource_effect("resource.request", ctx, started, &format!("fs.{name}"));
        Ok(result)
    }

    fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        self.budget.consume_fuel(amount, span)
    }

    fn check_stack(&self, depth: usize, span: Option<Span>) -> Result<(), RuntimeError> {
        self.budget.check_stack(depth, span)
    }

    fn check_deadline(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        self.budget.check_deadline(span)
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    fn trace(&self) -> &[TraceEntry] {
        self.trace.entries()
    }

    fn record_semantic(
        &mut self,
        kind: &str,
        function_id: Option<&str>,
        module_id: Option<&str>,
        span: Option<Span>,
        detail: Option<&str>,
    ) {
        let estimate = TraceState::estimate(kind, function_id, module_id, detail, None);
        if !self.trace_state.accept(estimate) {
            return;
        }
        let seq = self.next_seq();
        self.semantic_events.push(SemanticEvent {
            seq,
            kind: kind.to_string(),
            function_id: function_id.map(str::to_string),
            module_id: module_id.map(str::to_string),
            span,
            detail: detail.map(str::to_string),
        });
    }

    fn semantic_events(&self) -> &[SemanticEvent] {
        &self.semantic_events
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
