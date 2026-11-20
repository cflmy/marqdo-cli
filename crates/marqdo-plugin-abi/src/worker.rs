//! Isolated plugin worker transport (length-prefixed JSON frames).
//!
//! The worker is the same core binary invoked with the hidden
//! `__plugin-worker` command. It loads one native plugin in a child process and
//! serves invoke frames over stdin/stdout. This isolates crashes and provides a
//! process boundary; it is **not** an OS sandbox and does not make arbitrary
//! untrusted native code safe.

use std::io::{Read, Write};
use std::path::Path;

use serde_json::{json, Value};

use crate::error::PluginError;
use crate::loader::LoadedPlugin;
use crate::manifest::{PluginKind, PluginManifest};
use crate::wire::{WireRequest, WireResponse};

/// Hidden CLI command name used to start a worker.
pub const WORKER_COMMAND: &str = "__plugin-worker";
/// Maximum frame size accepted by either side.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Read one length-prefixed frame (4-byte big-endian length + JSON).
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Value, PluginError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length).map_err(|error| {
        PluginError::invoke(
            "plugin.crashed",
            format!("worker frame read failed: {error}"),
        )
    })?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(PluginError::wire(
            "worker_frame_invalid",
            format!("worker frame length {length} is invalid"),
        ));
    }
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes).map_err(|error| {
        PluginError::invoke(
            "plugin.crashed",
            format!("worker frame read failed: {error}"),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        PluginError::wire(
            "worker_frame_invalid",
            format!("worker frame is not valid JSON: {error}"),
        )
    })
}

/// Write one length-prefixed frame.
pub fn write_frame<W: Write>(writer: &mut W, value: &Value) -> Result<(), PluginError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        PluginError::wire(
            "worker_encode_failed",
            format!("cannot encode worker frame: {error}"),
        )
    })?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(PluginError::wire(
            "worker_frame_too_large",
            format!("worker frame is {} bytes", bytes.len()),
        ));
    }
    writer
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|()| writer.write_all(&bytes))
        .and_then(|()| writer.flush())
        .map_err(|error| {
            PluginError::invoke(
                "plugin.crashed",
                format!("worker frame write failed: {error}"),
            )
        })
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
    let mut plugin = match LoadedPlugin::open(&native_manifest, base_dir) {
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
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    if let Err(error) = write_frame(&mut stdout, &json!({"kind": "ready", "exports": exports})) {
        eprintln!("worker: {}: {}", error.code, error.message);
        return 1;
    }
    loop {
        let frame = match read_frame(&mut stdin) {
            Ok(frame) => frame,
            Err(_) => return 0,
        };
        match frame.get("kind").and_then(Value::as_str) {
            Some("shutdown") => return 0,
            Some("invoke") => {
                let Some(request_value) = frame.get("request") else {
                    let _ = write_frame(
                        &mut stdout,
                        &json!({"kind":"error","code":"worker_request_invalid","message":"missing request"}),
                    );
                    continue;
                };
                let request_bytes = match serde_json::to_vec(request_value) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = write_frame(
                            &mut stdout,
                            &json!({"kind":"error","code":"worker_request_invalid","message":error.to_string()}),
                        );
                        continue;
                    }
                };
                let request = match WireRequest::from_json_bytes(&request_bytes) {
                    Ok(request) => request,
                    Err(error) => {
                        let _ = write_frame(
                            &mut stdout,
                            &json!({"kind":"error","code":error.code,"message":error.message}),
                        );
                        continue;
                    }
                };
                match plugin.invoke(&request) {
                    Ok(response) => {
                        let body = response
                            .to_json_bytes()
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .unwrap_or_else(|| json!({"wire_version":1,"kind":"response"}));
                        if write_frame(&mut stdout, &json!({"kind":"response","response":body})).is_err() {
                            return 1;
                        }
                    }
                    Err(error) => {
                        if write_frame(
                            &mut stdout,
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
                if write_frame(
                    &mut stdout,
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

/// Build a successful response from an embedded wire response value.
pub fn response_from_frame(frame: &Value) -> Result<WireResponse, PluginError> {
    let body = frame
        .get("response")
        .ok_or_else(|| PluginError::wire("worker_response_invalid", "missing response body"))?;
    let bytes = serde_json::to_vec(body)
        .map_err(|error| PluginError::wire("worker_response_invalid", error.to_string()))?;
    WireResponse::from_json_bytes(&bytes)
}
