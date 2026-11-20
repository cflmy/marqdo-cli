//! CapabilityHost: the only entry point for side effects.
//!
//! The runtime never touches files, processes or the math kernel directly; it
//! calls [`Host::invoke`]. Authorization is checked here, at the point of
//! execution, not at parse/compile time.

use std::time::{Duration, Instant};

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
    output: Vec<u8>,
    trace_id: u64,
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
            output: Vec::new(),
            trace_id,
        }
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
            HostOp::PluginInvoke => Err(RuntimeError::new(
                ErrorCategory::KernelUnavailable,
                ctx.span,
                "no plugin runtime is loaded in this build",
            )),
        }
    }

    fn op_print(
        &mut self,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
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
