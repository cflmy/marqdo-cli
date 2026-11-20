//! Math and foreign kernel adapters.
//!
//! These functions are the only bridge between the capability host and the
//! native math / foreign crates. Every operation is capability-checked before
//! the kernel is touched; kernel failures are converted to structured runtime
//! errors, never panics.

use std::collections::BTreeMap;
use std::time::Duration;

use marqdo_value::{to_wire, ErrorCategory, FormulaValue, RuntimeError, Value};

use crate::context::ExecutionContext;
use crate::host::{arity, expect_text, HostOp, StdHost};

pub(crate) fn math(
    host: &mut StdHost,
    op: HostOp,
    args: Vec<Value>,
    ctx: &ExecutionContext,
) -> Result<Value, RuntimeError> {
    if !host.capabilities.math {
        return Err(denied(ctx, "the math capability was not granted"));
    }
    let span = ctx.span;
    match op {
        HostOp::MathParse => {
            let source = expect_text(&args, 0, "math.parse source", span)?;
            let formula = marqdo_math::parse(&source).map_err(|error| math_error(error, span))?;
            Ok(Value::Formula(FormulaValue {
                latex: formula.source,
                unit: None,
                display: false,
                span,
            }))
        }
        HostOp::MathEval => {
            if args.len() < 2 {
                return Err(arity(span, "math.eval", 2, args.len()));
            }
            let formula = formula_source(&args[0], span)?;
            let bindings = math_bindings(&args[1], span)?;
            let target = match args.get(2) {
                Some(Value::Text(target)) => Some(target.clone()),
                Some(Value::None) | None => None,
                Some(other) => {
                    return Err(RuntimeError::type_error(
                        span,
                        format!("math.eval target must be Text, got {}", other.type_name()),
                    ))
                }
            };
            let parsed = marqdo_math::parse(&formula).map_err(|error| math_error(error, span))?;
            marqdo_math::evaluate(&parsed, &bindings, target.as_deref())
                .map_err(|error| math_error(error, span))
        }
        HostOp::MathSubstitute => {
            if args.len() < 2 {
                return Err(arity(span, "math.substitute", 2, args.len()));
            }
            let source = formula_source(&args[0], span)?;
            let bindings = math_bindings(&args[1], span)?;
            let parsed = marqdo_math::parse(&source).map_err(|error| math_error(error, span))?;
            let substituted = marqdo_math::substitute(&parsed, &bindings)
                .map_err(|error| math_error(error, span))?;
            Ok(Value::Formula(FormulaValue {
                latex: substituted.source,
                unit: None,
                display: false,
                span,
            }))
        }
        HostOp::MathSimplify | HostOp::MathDifferentiate => Err(RuntimeError::new(
            ErrorCategory::MathUnsupported,
            span,
            format!(
                "`{}` is not part of the supported math subset in this build",
                op.name()
            ),
        )),
        _ => Err(RuntimeError::new(
            ErrorCategory::Internal,
            span,
            "unexpected math operation",
        )),
    }
}

fn formula_source(value: &Value, span: Option<marqdo_span::Span>) -> Result<String, RuntimeError> {
    match value {
        Value::Formula(formula) => Ok(formula.latex.clone()),
        Value::Text(text) => Ok(text.clone()),
        other => Err(RuntimeError::type_error(
            span,
            format!("expected Formula or Text, got {}", other.type_name()),
        )),
    }
}

fn math_bindings(
    value: &Value,
    span: Option<marqdo_span::Span>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    match value {
        Value::Map(map) => {
            let mut bindings = BTreeMap::new();
            for (name, value) in map {
                match value {
                    Value::Int(_) | Value::Float(_) => {
                        bindings.insert(name.clone(), value.clone());
                    }
                    other => {
                        return Err(RuntimeError::type_error(
                            span,
                            format!(
                                "math binding `{name}` must be Int or Float, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(bindings)
        }
        other => Err(RuntimeError::type_error(
            span,
            format!("math bindings must be a Map, got {}", other.type_name()),
        )),
    }
}

fn math_error(error: marqdo_math::MathError, span: Option<marqdo_span::Span>) -> RuntimeError {
    use marqdo_math::MathErrorKind;
    let category = match error.kind {
        MathErrorKind::Parse | MathErrorKind::Unsupported => ErrorCategory::MathUnsupported,
        MathErrorKind::UndefinedVariable => ErrorCategory::UnknownSymbol,
        MathErrorKind::Type => ErrorCategory::Type,
    };
    let mut message = error.message;
    if let Some(position) = error.position {
        message = format!("{message} (at byte {position} of the formula)");
    }
    RuntimeError::new(category, span, message)
}

pub(crate) fn foreign(
    host: &mut StdHost,
    op: HostOp,
    args: Vec<Value>,
    ctx: &ExecutionContext,
) -> Result<Value, RuntimeError> {
    host.capabilities.check_foreign(ctx.span)?;
    let span = ctx.span;
    let Some(Value::Code(code)) = args.first() else {
        return Err(RuntimeError::type_error(
            span,
            "foreign kernel expects a Code value from `ref(\"#代码块\")`",
        ));
    };
    if code.language.trim().is_empty() {
        return Err(RuntimeError::new(
            ErrorCategory::KernelUnavailable,
            span,
            "code fence has no language, so no kernel can be selected",
        ));
    }
    let input = match args.get(1) {
        None | Some(Value::None) => None,
        Some(value) => Some(to_wire(value)),
    };
    let request = marqdo_foreign::RunRequest {
        language: code.language.clone(),
        source: code.source.clone(),
        input,
        timeout: host.foreign_timeout,
        max_output_bytes: host.foreign_output_limit,
    };
    match op {
        HostOp::ForeignRun => {
            let result = marqdo_foreign::run_with_registry(&request, &host.kernel_registry)
                .map_err(|error| foreign_error(error, span))?;
            Ok(foreign_result_map(result))
        }
        HostOp::ForeignCall => {
            let result = marqdo_foreign::call_with_registry(&request, &host.kernel_registry)
                .map_err(|error| foreign_error(error, span))?;
            if result.status != marqdo_foreign::RunStatus::Success {
                return Err(RuntimeError::new(
                    ErrorCategory::ForeignFailure,
                    span,
                    format!(
                        "foreign call failed (status {:?}): {}",
                        result.status, result.stderr
                    ),
                ));
            }
            let value = result.value.ok_or_else(|| {
                RuntimeError::new(
                    ErrorCategory::ForeignFailure,
                    span,
                    "foreign call returned no typed value",
                )
            })?;
            if crate::handles::contains_plugin_handle(&value) {
                return Err(RuntimeError::new(
                    ErrorCategory::PermissionDenied,
                    span,
                    "plugin_handle.untrusted: foreign kernel output may not mint host handles",
                ));
            }
            Ok(value)
        }
        _ => Err(RuntimeError::new(
            ErrorCategory::Internal,
            span,
            "unexpected foreign operation",
        )),
    }
}

fn foreign_result_map(result: marqdo_foreign::RunResult) -> Value {
    let mut map = BTreeMap::new();
    map.insert(
        "status".to_string(),
        Value::text(match result.status {
            marqdo_foreign::RunStatus::Success => "success",
            marqdo_foreign::RunStatus::Failed => "failed",
            marqdo_foreign::RunStatus::TimedOut => "timed_out",
        }),
    );
    map.insert("stdout".to_string(), Value::text(result.stdout));
    map.insert("stderr".to_string(), Value::text(result.stderr));
    map.insert(
        "exit_code".to_string(),
        match result.exit_code {
            Some(code) => Value::int(code as i64),
            None => Value::None,
        },
    );
    map.insert(
        "duration_ms".to_string(),
        Value::int(result.duration.as_millis() as i64),
    );
    map.insert(
        "value".to_string(),
        match result.value {
            Some(value) => value,
            None => Value::None,
        },
    );
    Value::Map(map)
}

fn foreign_error(
    error: marqdo_foreign::ForeignError,
    span: Option<marqdo_span::Span>,
) -> RuntimeError {
    use marqdo_foreign::ForeignError;
    let (category, message) = match error {
        ForeignError::KernelUnavailable { language, hint } => (
            ErrorCategory::KernelUnavailable,
            format!("foreign kernel `{language}` is unavailable: {hint}"),
        ),
        ForeignError::Spawn { message } => (ErrorCategory::HostError, message),
        ForeignError::Io { message } => (ErrorCategory::HostError, message),
        ForeignError::Timeout { seconds } => (
            ErrorCategory::ResourceExhausted,
            format!("foreign kernel exceeded its {seconds}s timeout"),
        ),
        ForeignError::Protocol { message } => (ErrorCategory::ForeignFailure, message),
        ForeignError::OutputTooLarge { limit } => (
            ErrorCategory::ResourceExhausted,
            format!("foreign kernel exceeded the {limit} byte output limit"),
        ),
    };
    RuntimeError::new(category, span, message)
}

fn denied(ctx: &ExecutionContext, message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::PermissionDenied, ctx.span, message)
}

/// Default per-call foreign timeout used by the CLI.
pub const DEFAULT_FOREIGN_TIMEOUT: Duration = Duration::from_secs(10);
