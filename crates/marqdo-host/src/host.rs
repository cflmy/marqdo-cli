//! CapabilityHost: the only entry point for side effects.
//!
//! The runtime never touches files, processes or the math kernel directly; it
//! calls [`Host::invoke`]. Authorization is checked here, at the point of
//! execution, not at parse/compile time.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use marqdo_plugin_abi::{ErrorKind as PluginErrorKind, LoadedPlugin, PluginError, PluginManifest};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};

use crate::budget::Budget;
use crate::capability::{Capabilities, CapabilityAudit, CapabilityBroker};
use crate::context::ExecutionContext;
use crate::fs::ProjectFs;
use crate::trace::{EffectTrace, TraceEntry};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode};

/// Host operations reachable from the language surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOp {
    /// Write a value to program stdout.
    Print,
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
            HostOp::Print => "print",
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
    /// Shared audit log for plugin capability requests.
    broker_audit: Arc<Mutex<Vec<CapabilityAudit>>>,
}

struct PluginSlot {
    /// Package name used for capability grants.
    package: String,
    /// Artifact/manifest digest that produced this registration.
    manifest_digest: String,
    /// Complete export name -> declared signature.
    exports: BTreeMap<String, String>,
    plugin: LoadedPlugin,
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
            broker_audit: Arc::new(Mutex::new(Vec::new())),
        }
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
        if !trusted {
            let mut error = RuntimeError::new(
                ErrorCategory::PermissionDenied,
                None,
                "loading an in-process native plugin requires explicit trust",
            );
            error.effect_trace.push(
                "hint: no isolated plugin worker transport is implemented in this build; in-process native code shares the host address space"
                    .to_string(),
            );
            return Err(error);
        }
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
        let mut broker =
            CapabilityBroker::with_audit(self.capabilities.clone(), Arc::clone(&self.broker_audit));
        for capability in &self.vendor_capabilities {
            broker.allow_vendor(capability.clone());
        }
        let package = manifest.plugin.name.clone();
        let options = marqdo_plugin_abi::LoadOptions {
            host_target: marqdo_plugin_abi::HostTarget::current(),
            capability_handler: Some(Box::new(move |capability, request| {
                broker.authorize(&package, capability, request)
            })),
        };
        let loaded =
            LoadedPlugin::open_with(manifest, base_dir, options).map_err(map_plugin_error)?;
        let index = self.plugins.len();
        self.plugins.push(PluginSlot {
            package: manifest.plugin.name.clone(),
            manifest_digest,
            exports: exports.clone(),
            plugin: loaded,
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

    /// Enable effect tracing.
    pub fn with_trace(mut self) -> Self {
        self.trace = EffectTrace::enabled();
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

    fn dispatch(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        match op {
            HostOp::Print => self.op_print(args, ctx),
            HostOp::FsRead => self.op_fs_read(args, ctx),
            HostOp::FsWrite => self.op_fs_write(args, ctx),
            HostOp::MathParse
            | HostOp::MathEval
            | HostOp::MathSubstitute
            | HostOp::MathSimplify
            | HostOp::MathDifferentiate => crate::kernels::math(self, op, args, ctx),
            HostOp::ForeignRun | HostOp::ForeignCall => {
                crate::kernels::foreign(self, op, args, ctx)
            }
            HostOp::PluginInvoke => self.op_plugin_invoke(args, ctx),
        }
    }

    fn op_print(
        &mut self,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        if !self.capabilities.console {
            return Err(RuntimeError::new(
                ErrorCategory::PermissionDenied,
                ctx.span,
                "program output (console) capability was not granted",
            ));
        }
        if args.len() != 1 {
            return Err(arity(ctx.span, "打印", 1, args.len()));
        }
        let text = format!("{}\n", args[0].display());
        self.budget.consume_output(text.len(), ctx.span)?;
        self.output.extend_from_slice(text.as_bytes());
        Ok(Value::Unit)
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
        let started = Instant::now();
        let result = self.dispatch(op, args, ctx);
        let summary = match &result {
            Ok(_) => "ok".to_string(),
            Err(error) => error.code().to_string(),
        };
        self.trace.record(
            op.name(),
            ctx.module_id.clone(),
            ctx.span,
            started.elapsed(),
            summary,
        );
        result
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
