//! CapabilityHost: the only entry point for side effects.
//!
//! The runtime never touches files, processes or the math kernel directly; it
//! calls [`Host::invoke`]. Authorization is checked here, at the point of
//! execution, not at parse/compile time.

use std::time::{Duration, Instant};

use marqdo_plugin_abi::{ErrorKind as PluginErrorKind, LoadedPlugin, PluginError, PluginManifest};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};

use crate::budget::Budget;
use crate::capability::Capabilities;
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
    /// Explicitly trusted, in-process native plugins keyed by namespace.
    plugins: Vec<PluginSlot>,
}

struct PluginSlot {
    /// Call namespace (prefix of the export name, e.g. `solver`).
    namespace: String,
    /// Package name used for capability grants.
    package: String,
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
        }
    }

    /// Load a native plugin in-process after an explicit trust decision.
    ///
    /// In-process native code shares the host address space and is **not**
    /// sandboxed; this build has no isolated worker transport. The caller must
    /// pass `trusted = true` and grant the plugin capability. Returns the call
    /// namespace derived from the plugin's export names.
    pub fn load_plugin(
        &mut self,
        manifest: &PluginManifest,
        base_dir: &std::path::Path,
        trusted: bool,
    ) -> Result<String, RuntimeError> {
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
        let namespace = manifest
            .plugin
            .exports
            .first()
            .and_then(|export| {
                export
                    .name
                    .split_once('.')
                    .map(|(namespace, _)| namespace.to_string())
            })
            .ok_or_else(|| {
                RuntimeError::new(
                    ErrorCategory::PluginAbi,
                    None,
                    "plugin manifest declares no namespaced exports",
                )
            })?;
        let loaded = LoadedPlugin::open_with(
            manifest,
            base_dir,
            marqdo_plugin_abi::LoadOptions::default(),
        )
        .map_err(map_plugin_error)?;
        self.plugins.push(PluginSlot {
            namespace: namespace.clone(),
            package: manifest.plugin.name.clone(),
            plugin: loaded,
        });
        Ok(namespace)
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
        if args.len() != 3 {
            return Err(arity(ctx.span, "plugin.invoke", 3, args.len()));
        }
        let Value::Text(namespace) = &args[0] else {
            return Err(RuntimeError::type_error(
                ctx.span,
                "plugin namespace must be Text",
            ));
        };
        let Value::Text(export) = &args[1] else {
            return Err(RuntimeError::type_error(
                ctx.span,
                "plugin export must be Text",
            ));
        };
        let Value::Map(named_args) = &args[2] else {
            return Err(RuntimeError::type_error(
                ctx.span,
                "plugin call arguments must be named",
            ));
        };
        let slot = self
            .plugins
            .iter_mut()
            .find(|slot| &slot.namespace == namespace)
            .ok_or_else(|| {
                RuntimeError::new(
                    ErrorCategory::UnknownSymbol,
                    ctx.span,
                    format!("no plugin namespace `{namespace}` is loaded"),
                )
            })?;
        self.capabilities
            .check_plugin(&slot.package.clone(), ctx.span)?;
        let function = format!("{namespace}.{export}");
        let mut request = marqdo_plugin_abi::WireRequest::new(function.clone());
        for (name, value) in named_args {
            request = request.with_arg(name.clone(), value);
        }
        let response = slot.plugin.invoke(&request).map_err(map_plugin_error)?;
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
