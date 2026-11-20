//! Entry-point plumbing used by [`mq_plugin_entry!`](crate::mq_plugin_entry).
//!
//! These functions are the "runtime" of the SDK macro. They are public so the
//! macro can reference them through `$crate` from a plugin crate, but they are
//! not intended for direct use.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicPtr, Ordering};

use marqdo_plugin_abi::abi::{
    check_host_api, result, MqBytes, MqCallContext, MqHostApi, MqOwnedBytes, MqPluginApi,
    MqPluginInvoke, ABI_MAJOR, ABI_MINOR, MAX_ERROR_WIRE_BYTES, MAX_EXPORT_NAME_BYTES,
    MAX_RESULT_WIRE_BYTES,
};
use marqdo_plugin_abi::{PluginError, WireResponse};

use crate::bytes::{release_host_buffer, HostBuffer};
use crate::call::CallContext;
use crate::HandlerFn;

/// Host API table captured by the entry point for the plugin lifetime.
static CURRENT_HOST: AtomicPtr<MqHostApi> = AtomicPtr::new(std::ptr::null_mut());

/// The host API table captured at load time, if a plugin is loaded.
pub(crate) fn current_host() -> Option<&'static MqHostApi> {
    let pointer = CURRENT_HOST.load(Ordering::SeqCst);
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the pointer was stored from a validated host table and the host
    // guarantees it is valid for the plugin lifetime (single-threaded model).
    Some(unsafe { &*pointer })
}

/// Initialize the ABI tables for the generated entry point.
///
/// # Safety
///
/// `host` and `plugin` must be the pointers provided by the host to
/// `mq_plugin_entry_v1` and must satisfy the header contract.
pub unsafe fn initialize(
    host: *const MqHostApi,
    plugin: *mut MqPluginApi,
    metadata: &'static [u8],
) -> i32 {
    if plugin.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: host pointer comes from the host and is checked before use.
    let negotiated = match unsafe { check_host_api(host) } {
        Ok(version) => version,
        Err(_) => return result::ABI_MISMATCH,
    };
    if negotiated.major != ABI_MAJOR {
        return result::ABI_MISMATCH;
    }
    CURRENT_HOST.store(host as *mut MqHostApi, Ordering::SeqCst);
    // SAFETY: plugin table is host-provided, writable and valid for this call.
    unsafe {
        (*plugin).struct_size = std::mem::size_of::<MqPluginApi>() as u32;
        (*plugin).abi_major = ABI_MAJOR;
        (*plugin).abi_minor = ABI_MINOR;
        (*plugin).plugin_metadata_wire = MqBytes::from_slice(metadata);
    }
    result::OK
}

/// Register one export through the host table.
///
/// # Safety
///
/// `host` must be the table validated by [`initialize`]; both strings must be
/// valid UTF-8 dotted export names/signatures declared in `plugin.yaml`.
pub unsafe fn register_export(
    host: *const MqHostApi,
    name: &str,
    signature: &str,
    invoke: MqPluginInvoke,
) -> i32 {
    if host.is_null() || name.is_empty() || name.len() > MAX_EXPORT_NAME_BYTES {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: host pointer is non-NULL and valid for the plugin lifetime.
    let host = unsafe { &*host };
    let signature_wire = match serde_json::to_vec(&serde_json::json!({"signature": signature})) {
        Ok(bytes) => bytes,
        Err(_) => return result::BAD_WIRE,
    };
    // SAFETY: host table was validated; strings outlive the call.
    unsafe {
        (host.register_function)(
            host.host_data,
            MqBytes::from_slice(name.as_bytes()),
            MqBytes::from_slice(&signature_wire),
            invoke,
        )
    }
}

/// Dispatch one invocation to a plugin handler and write the response.
///
/// # Safety
///
/// Must be called as the body of a `MqPluginInvoke` callback with the buffers
/// provided by the host for that invocation.
pub unsafe fn dispatch(
    handler: HandlerFn,
    ctx: *mut MqCallContext,
    args: MqBytes,
    result_out: *mut MqOwnedBytes,
    error_out: *mut MqOwnedBytes,
) -> i32 {
    if result_out.is_null() || error_out.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: output pointers are non-NULL host-provided out-parameters.
    unsafe {
        *result_out = MqOwnedBytes::EMPTY;
        *error_out = MqOwnedBytes::EMPTY;
    }
    let Some(host) = current_host() else {
        return result::INTERNAL;
    };
    // SAFETY: borrowed request buffer is valid for this callback.
    let args_bytes = match unsafe { args.as_slice() } {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => return result::INVALID_ARGUMENT,
    };

    // SAFETY: context pointer is the host-minted context for this invocation.
    let mut context = unsafe { CallContext::new(ctx, host) };
    let outcome = catch_unwind(AssertUnwindSafe(|| handler(&mut context, &args_bytes)));
    let response = match outcome {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(PluginError::invoke(
            "plugin_panic",
            "plugin handler panicked; the panic was caught at the ABI boundary",
        )),
    };

    match response {
        Ok(bytes) => {
            if bytes.len() > MAX_RESULT_WIRE_BYTES {
                return write_error(
                    host,
                    error_out,
                    &PluginError::wire(
                        "result_too_large",
                        format!(
                            "plugin result is {} bytes, limit is {MAX_RESULT_WIRE_BYTES}",
                            bytes.len()
                        ),
                    ),
                );
            }
            match HostBuffer::copy_from(host, &bytes) {
                Ok(buffer) => {
                    // SAFETY: result_out is a valid out-parameter.
                    unsafe { *result_out = buffer.into_raw() };
                    result::OK
                }
                Err(error) => write_error(host, error_out, &error),
            }
        }
        Err(error) => write_error(host, error_out, &error),
    }
}

fn write_error(host: &'static MqHostApi, error_out: *mut MqOwnedBytes, error: &PluginError) -> i32 {
    let encoded = match WireResponse::from_error(error).to_json_bytes() {
        Ok(bytes) if bytes.len() <= MAX_ERROR_WIRE_BYTES => bytes,
        _ => br#"{"wire_version":1,"kind":"response","error":{"kind":"invoke","code":"plugin_error","message":"plugin failed with an unencodable error"}}"#.to_vec(),
    };
    let code = match error.kind {
        marqdo_plugin_abi::ErrorKind::Wire => result::BAD_WIRE,
        marqdo_plugin_abi::ErrorKind::Capability => result::CAPABILITY,
        marqdo_plugin_abi::ErrorKind::Unsupported => result::UNSUPPORTED,
        _ => result::INTERNAL,
    };
    match HostBuffer::copy_from(host, &encoded) {
        Ok(buffer) => {
            // SAFETY: error_out is a valid out-parameter.
            unsafe { *error_out = buffer.into_raw() };
            code
        }
        Err(_) => result::OUT_OF_MEMORY,
    }
}

/// Free a host-owned buffer (used by generated code/tests).
///
/// # Safety
///
/// `buffer` must have been allocated by the host allocator and not freed yet.
pub unsafe fn free_host_buffer(host: &MqHostApi, buffer: MqOwnedBytes) {
    // SAFETY: caller guarantees the alloc/free pair.
    unsafe { release_host_buffer(host, buffer) };
}
