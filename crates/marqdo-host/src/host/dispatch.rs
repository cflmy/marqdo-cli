//! `Host::invoke` dispatch table.
//!
//! Child module of `host`: it may call the parent's private capability,
//! resource and observation helpers. `StdHost` remains the only host implementation.

use super::*;

pub(crate) fn invoke(
    host: &mut StdHost,
    op: HostOp,
    args: Vec<Value>,
    ctx: &ExecutionContext,
) -> Result<Value, RuntimeError> {
    // Addressable memory is language-local state. Handles must not reach a
    // plugin, foreign kernel or file adapter, where they could not be
    // interpreted safely (RFC-CM-0001 §8, ADR-0022 §4).
    if args.iter().any(Value::contains_memory) {
        return Err(RuntimeError::new(
            ErrorCategory::Type,
            ctx.span,
            format!(
                "Memory values cannot cross the host boundary (`{}`); copy bytes into \
                     Text explicitly first",
                op.name()
            ),
        ));
    }
    if args.iter().any(Value::contains_resource) {
        return Err(RuntimeError::new(
            ErrorCategory::Type,
            ctx.span,
            format!(
                "Resource handles cannot cross the generic host boundary (`{}`); use \
                     resource.* operations",
                op.name()
            ),
        ));
    }
    let result = host.dispatch(op, args, ctx);
    let result_code = match &result {
        Ok(_) => "ok",
        Err(error) => error.code(),
    };
    host.observe_effect(op.name(), result_code, ctx);
    result
}

pub(crate) fn dispatch(
    host: &mut StdHost,
    op: HostOp,
    args: Vec<Value>,
    ctx: &ExecutionContext,
) -> Result<Value, RuntimeError> {
    match op {
        HostOp::FsRead => host.op_fs_read(args, ctx),
        HostOp::FsWrite => host.op_fs_write(args, ctx),
        HostOp::ForeignRun | HostOp::ForeignCall => crate::kernels::foreign(host, op, args, ctx),
        HostOp::PluginInvoke => host.op_plugin_invoke(args, ctx),
    }
}
