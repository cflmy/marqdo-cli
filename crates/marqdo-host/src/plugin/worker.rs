//! Isolated plugin worker transport (length-prefixed JSON frames).
//!
//! The worker is the same core binary invoked with the hidden
//! `__plugin-worker` command. It loads one native plugin in a child process and
//! serves invoke frames over stdin/stdout. This isolates crashes and provides a
//! process boundary; it is **not** an OS sandbox and does not make arbitrary
//! untrusted native code safe.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::loader::{LoadOptions, LoadedPlugin};
use super::manifest::{PluginKind, PluginManifest};
use marqdo_plugin_abi::wire::{WireRequest, WireResponse};
use marqdo_plugin_abi::PluginError;

/// Hidden CLI command name used to start a worker.
pub const WORKER_COMMAND: &str = "__plugin-worker";
/// Maximum frame size accepted by either side.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Maximum capability requests a plugin may make during one invocation.
pub const MAX_CAPABILITY_REQUESTS: usize = 64;
/// Maximum serialized bytes of one capability request.
pub const MAX_CAPABILITY_REQUEST_BYTES: usize = 64 * 1024;

/// Read one length-prefixed frame (4-byte big-endian length + JSON).
///
/// `Ok(None)` means a clean EOF at a frame boundary. A partial header or
/// truncated payload is a protocol error, never EOF. Framing itself is shared
/// with other persistent workers; the plugin vocabulary stays in this module.
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Value>, PluginError> {
    marqdo_worker_ipc::read_frame(reader, MAX_FRAME_BYTES).map_err(frame_read_error)
}

/// Write one length-prefixed frame.
pub fn write_frame<W: Write>(writer: &mut W, value: &Value) -> Result<(), PluginError> {
    marqdo_worker_ipc::write_frame(writer, value, MAX_FRAME_BYTES).map_err(frame_write_error)
}

fn frame_read_error(error: marqdo_worker_ipc::FrameError) -> PluginError {
    match error {
        marqdo_worker_ipc::FrameError::Io(error) => PluginError::invoke(
            "plugin.crashed",
            format!("worker frame read failed: {error}"),
        ),
        other => PluginError::wire("worker_frame_invalid", other.to_string()),
    }
}

fn frame_write_error(error: marqdo_worker_ipc::FrameError) -> PluginError {
    match error {
        marqdo_worker_ipc::FrameError::Io(error) => PluginError::invoke(
            "plugin.crashed",
            format!("worker frame write failed: {error}"),
        ),
        marqdo_worker_ipc::FrameError::Encode(error) => {
            PluginError::wire("worker_encode_failed", error.to_string())
        }
        marqdo_worker_ipc::FrameError::Length(length) => PluginError::wire(
            "worker_frame_too_large",
            format!("worker frame is {length} bytes"),
        ),
        other => PluginError::wire("worker_frame_invalid", other.to_string()),
    }
}

/// Run one worker until shutdown/EOF. Returns a process exit code.
pub fn serve(manifest_path: &Path, base_dir: &Path) -> i32 {
    let manifest = match PluginManifest::load(manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("worker: {}: {}", error.code, error.message);
            return 2;
        }
    };
    // The worker is the isolated transport; the library itself is loaded
    // natively inside this child process.
    let mut native_manifest = manifest.clone();
    native_manifest.plugin.kind = PluginKind::Native;

    let stdin = Arc::new(Mutex::new(std::io::stdin()));
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let next_request_id = Arc::new(AtomicU64::new(1));
    let capability_calls = Arc::new(AtomicUsize::new(0));

    let handler_stdin = Arc::clone(&stdin);
    let handler_stdout = Arc::clone(&stdout);
    let handler_ids = Arc::clone(&next_request_id);
    let handler_calls = Arc::clone(&capability_calls);
    let capability_handler: super::loader::CapabilityHandler = Box::new(
        move |capability, request| {
            if handler_calls.fetch_add(1, Ordering::SeqCst) >= MAX_CAPABILITY_REQUESTS {
                return Err(PluginError::capability(
                    "capability.budget_exceeded",
                    format!(
                        "plugin exceeded the limit of {MAX_CAPABILITY_REQUESTS} capability requests per invocation"
                    ),
                ));
            }
            if request.len() > MAX_CAPABILITY_REQUEST_BYTES {
                return Err(PluginError::capability(
                    "capability.request_too_large",
                    format!(
                        "capability request is {} bytes; limit is {MAX_CAPABILITY_REQUEST_BYTES}",
                        request.len()
                    ),
                ));
            }
            let id = handler_ids.fetch_add(1, Ordering::SeqCst);
            let request_value: Value = serde_json::from_slice(request).map_err(|error| {
                PluginError::capability(
                    "capability.request_invalid",
                    format!("capability request is not valid JSON: {error}"),
                )
            })?;
            {
                let mut out = handler_stdout.lock().map_err(|_| {
                    PluginError::invoke("plugin.crashed", "worker stdout lock is poisoned")
                })?;
                write_frame(
                    &mut *out,
                    &json!({
                        "kind": "capability_request",
                        "id": id,
                        "capability": capability,
                        "request": request_value,
                    }),
                )?;
            }
            {
                let frame = {
                    let mut input = handler_stdin.lock().map_err(|_| {
                        PluginError::invoke("plugin.crashed", "worker stdin lock is poisoned")
                    })?;
                    read_frame(&mut *input).map_err(|error| {
                        PluginError::invoke(
                            error.code.as_str(),
                            format!("worker capability reply failed: {}", error.message),
                        )
                    })?
                };
                let Some(frame) = frame else {
                    return Err(PluginError::invoke(
                        "plugin.crashed",
                        "host closed the worker during a capability request",
                    ));
                };
                if frame.get("id").and_then(Value::as_u64) != Some(id) {
                    return Err(PluginError::wire(
                        "worker_frame_invalid",
                        "capability reply id does not match the request",
                    ));
                }
                match frame.get("kind").and_then(Value::as_str) {
                    Some("capability_reply") => {
                        let data = frame.get("data").and_then(Value::as_str).ok_or_else(|| {
                            PluginError::wire(
                                "worker_frame_invalid",
                                "capability reply has no text data",
                            )
                        })?;
                        Ok(data.as_bytes().to_vec())
                    }
                    Some("capability_error") => Err(PluginError::capability(
                        frame
                            .get("code")
                            .and_then(Value::as_str)
                            .unwrap_or("capability.unavailable"),
                        frame
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("capability request failed")
                            .to_string(),
                    )),
                    _ => Err(PluginError::wire(
                        "worker_frame_invalid",
                        "expected a capability reply frame",
                    )),
                }
            }
        },
    );
    let options = LoadOptions {
        host_target: super::artifact::HostTarget::current(),
        capability_handler: Some(capability_handler),
    };
    let mut plugin = match LoadedPlugin::open_with(&native_manifest, base_dir, options) {
        Ok(plugin) => plugin,
        Err(error) => {
            eprintln!("worker: {}: {}", error.code, error.message);
            return 2;
        }
    };
    let exports: Vec<Value> = plugin
        .exports()
        .map(|export| json!({"name": export.name(), "signature": export.signature()}))
        .collect();
    if send_frame(&stdout, &json!({"kind": "ready", "exports": exports})).is_err() {
        return 1;
    }
    loop {
        let frame = match receive_frame(&stdin) {
            Ok(Some(frame)) => frame,
            Ok(None) => return 0,
            Err(error) => {
                eprintln!("worker: {}: {}", error.code, error.message);
                return 1;
            }
        };
        match frame.get("kind").and_then(Value::as_str) {
            Some("shutdown") => return 0,
            Some("invoke") => {
                let Some(request_value) = frame.get("request") else {
                    let _ = send_frame(
                        &stdout,
                        &json!({"kind":"error","code":"worker_request_invalid","message":"missing request"}),
                    );
                    continue;
                };
                let request_bytes = match serde_json::to_vec(request_value) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = send_frame(
                            &stdout,
                            &json!({"kind":"error","code":"worker_request_invalid","message":error.to_string()}),
                        );
                        continue;
                    }
                };
                let request = match WireRequest::from_json_bytes(&request_bytes) {
                    Ok(request) => request,
                    Err(error) => {
                        let _ = send_frame(
                            &stdout,
                            &json!({"kind":"error","code":error.code,"message":error.message}),
                        );
                        continue;
                    }
                };
                capability_calls.store(0, Ordering::SeqCst);
                match plugin.invoke(&request) {
                    Ok(response) => {
                        let body = response
                            .to_json_bytes()
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .unwrap_or_else(|| json!({"wire_version":1,"kind":"response"}));
                        if send_frame(&stdout, &json!({"kind":"response","response":body})).is_err() {
                            return 1;
                        }
                    }
                    Err(error) => {
                        if send_frame(
                            &stdout,
                            &json!({"kind":"error","code":error.code,"message":error.message,"error_kind":error.kind.as_str()}),
                        )
                        .is_err()
                        {
                            return 1;
                        }
                    }
                }
            }
            _ => {
                if send_frame(
                    &stdout,
                    &json!({"kind":"error","code":"worker_frame_invalid","message":"unknown frame kind"}),
                )
                .is_err()
                {
                    return 1;
                }
            }
        }
    }
}

fn send_frame(stdout: &Arc<Mutex<std::io::Stdout>>, value: &Value) -> Result<(), PluginError> {
    let mut out = stdout
        .lock()
        .map_err(|_| PluginError::invoke("plugin.crashed", "worker stdout lock is poisoned"))?;
    write_frame(&mut *out, value)
}

fn receive_frame(stdin: &Arc<Mutex<std::io::Stdin>>) -> Result<Option<Value>, PluginError> {
    let mut input = stdin
        .lock()
        .map_err(|_| PluginError::invoke("plugin.crashed", "worker stdin lock is poisoned"))?;
    read_frame(&mut *input)
}

pub fn response_from_frame(frame: &Value) -> Result<WireResponse, PluginError> {
    let body = frame
        .get("response")
        .ok_or_else(|| PluginError::wire("worker_response_invalid", "missing response body"))?;
    let bytes = serde_json::to_vec(body)
        .map_err(|error| PluginError::wire("worker_response_invalid", error.to_string()))?;
    WireResponse::from_json_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip_and_clean_eof() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &json!({"kind": "shutdown"})).unwrap();
        let mut cursor = Cursor::new(bytes.clone());
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Some(json!({"kind": "shutdown"}))
        );
        assert_eq!(read_frame(&mut cursor).unwrap(), None);
    }

    #[test]
    fn partial_header_is_an_error_not_eof() {
        let mut cursor = Cursor::new(vec![0_u8, 0]);
        let error = read_frame(&mut cursor).unwrap_err();
        assert_eq!(error.code, "worker_frame_invalid");
    }

    #[test]
    fn oversized_and_zero_length_frames_are_rejected() {
        for length in [0_u32, (MAX_FRAME_BYTES as u32) + 1] {
            let mut cursor = Cursor::new(length.to_be_bytes().to_vec());
            let error = read_frame(&mut cursor).unwrap_err();
            assert_eq!(error.code, "worker_frame_invalid");
        }
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let mut bytes = 10_u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        let error = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(error.code, "worker_frame_invalid");
    }
}
