//! Foreign-kernel adapters.
//!
//! This module is the bridge between the capability host and the native foreign
//! kernel crate. Every operation is capability-checked before the kernel is
//! touched; kernel failures are converted to structured runtime errors, never
//! panics. Domain math is not a host kernel (ADR-0061): it belongs to a pure
//! `.mq.md` package or an explicitly selected ABI backend.

use std::collections::BTreeMap;
use std::time::Duration;

use marqdo_value::{to_wire, ErrorCategory, RuntimeError, Value};

use crate::context::ExecutionContext;
use crate::host::{HostOp, StdHost};

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
        ForeignError::Integrity {
            provider,
            expected,
            actual,
        } => (
            ErrorCategory::KernelUnavailable,
            format!(
                "foreign kernel `{provider}` failed integrity verification: expected {expected}, got {actual}"
            ),
        ),
        ForeignError::EnvironmentIntegrity {
            environment,
            expected,
            actual,
        } => (
            ErrorCategory::KernelUnavailable,
            format!(
                "foreign kernel environment `{environment}` failed integrity verification: expected {expected}, got {actual}"
            ),
        ),
        ForeignError::OutputTooLarge { limit } => (
            ErrorCategory::ResourceExhausted,
            format!("foreign kernel exceeded the {limit} byte output limit"),
        ),
    };
    RuntimeError::new(category, span, message)
}

/// Default per-call foreign timeout used by the CLI.
pub const DEFAULT_FOREIGN_TIMEOUT: Duration = Duration::from_secs(10);
