//! Host side of the isolated plugin worker transport (ADR-0010).
//!
//! The worker is a child process running the same core binary with the hidden
//! `__plugin-worker` command. It isolates crashes and keeps native code out of
//! the interpreter process. A plain process is **not** an OS sandbox; strict
//! mode still refuses without a real isolation adapter.
//!
//! Framing, bounded reads, deadlines and teardown come from
//! [`marqdo_worker_ipc`]. The invoke/capability vocabulary, capability limit and
//! plugin error codes remain owned here: a shared transport never shares
//! capabilities.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::plugin::worker::{
    response_from_frame, MAX_CAPABILITY_REQUESTS, MAX_CAPABILITY_REQUEST_BYTES, MAX_FRAME_BYTES,
    WORKER_COMMAND,
};
use crate::plugin::{CapabilityHandler, PluginManifest};
use marqdo_plugin_abi::{PluginError, WireRequest, WireResponse};
use marqdo_worker_ipc::{Connection, FrameError, TransportError};
use serde_json::json;

/// A native plugin running in an isolated worker process.
pub struct WorkerPlugin {
    connection: Connection,
    exports: BTreeMap<String, String>,
    dead: bool,
    capability_handler: Option<CapabilityHandler>,
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

impl std::fmt::Debug for WorkerPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerPlugin")
            .field("pid", &self.connection.id())
            .field("exports", &self.exports.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl WorkerPlugin {
    /// Spawn a worker for `manifest` and wait for its ready frame.
    pub fn spawn(
        manifest: &PluginManifest,
        base_dir: &Path,
        capability_handler: Option<CapabilityHandler>,
    ) -> Result<Self, PluginError> {
        // The worker receives an explicit serialized manifest, so it does not
        // depend on the manifest file being named `plugin.yaml`.
        let manifest_path = std::env::temp_dir().join(format!(
            "marqdo-plugin-worker-manifest-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let manifest_bytes = serde_json::to_vec(manifest).map_err(|error| {
            PluginError::manifest(
                "worker_manifest_encode_failed",
                format!("cannot encode manifest for worker: {error}"),
            )
        })?;
        std::fs::write(&manifest_path, manifest_bytes).map_err(|error| {
            PluginError::load(
                "worker_manifest_unwritable",
                format!("cannot write worker manifest: {error}"),
            )
        })?;
        let executable = std::env::current_exe().map_err(|error| {
            PluginError::load(
                "worker_spawn_failed",
                format!("cannot find the current executable: {error}"),
            )
        })?;
        let child: Child = match Command::new(executable)
            .arg(WORKER_COMMAND)
            .arg(&manifest_path)
            .arg(base_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_file(&manifest_path);
                return Err(PluginError::load(
                    "worker_spawn_failed",
                    format!("cannot start plugin worker: {error}"),
                ));
            }
        };
        let connection = match Connection::attach(child, MAX_FRAME_BYTES) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = std::fs::remove_file(&manifest_path);
                return Err(PluginError::load(
                    "worker_spawn_failed",
                    format!("cannot start plugin worker transport: {error}"),
                ));
            }
        };
        let mut connection = connection;
        let ready = match connection.receive(Instant::now() + HANDSHAKE_TIMEOUT) {
            Ok(frame) => frame,
            Err(TransportError::Timeout) => {
                let _ = std::fs::remove_file(&manifest_path);
                return Err(PluginError::invoke(
                    "plugin.timeout",
                    "plugin worker handshake timed out",
                ));
            }
            Err(error) => {
                let _ = std::fs::remove_file(&manifest_path);
                return Err(transport_error(error));
            }
        };
        let _ = std::fs::remove_file(&manifest_path);
        if ready.get("kind").and_then(serde_json::Value::as_str) != Some("ready") {
            return Err(PluginError::load(
                "worker_handshake_failed",
                "worker did not report ready",
            ));
        }
        let mut exports = BTreeMap::new();
        if let Some(entries) = ready.get("exports").and_then(serde_json::Value::as_array) {
            for entry in entries {
                let Some(name) = entry.get("name").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let signature = entry
                    .get("signature")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                exports.insert(name.to_string(), signature);
            }
        }
        let declared: std::collections::BTreeSet<&str> = manifest
            .plugin
            .exports
            .iter()
            .map(|export| export.name.as_str())
            .collect();
        let actual: std::collections::BTreeSet<&str> = exports.keys().map(String::as_str).collect();
        if declared != actual {
            return Err(PluginError::manifest(
                "worker_export_mismatch",
                "worker exports do not match the manifest",
            ));
        }
        Ok(Self {
            connection,
            exports,
            dead: false,
            capability_handler,
        })
    }

    /// Invoke one export in the worker.
    pub fn invoke(&mut self, request: &WireRequest) -> Result<WireResponse, PluginError> {
        if self.dead {
            return Err(PluginError::invoke(
                "plugin.crashed",
                "plugin worker is no longer usable after a transport failure",
            ));
        }
        let request_value: serde_json::Value = serde_json::from_slice(&request.to_json_bytes()?)
            .map_err(|error| PluginError::wire("worker_encode_failed", error.to_string()))?;
        let deadline = Instant::now() + CALL_TIMEOUT;
        if let Err(error) = self.connection.send(
            json!({"kind": "invoke", "request": request_value}),
            deadline,
        ) {
            self.mark_dead();
            return Err(invocation_error(error));
        }
        let mut capability_requests = 0usize;
        loop {
            let frame = match self.connection.receive(deadline) {
                Ok(frame) => frame,
                Err(error) => {
                    self.mark_dead();
                    return Err(invocation_error(error));
                }
            };
            match frame.get("kind").and_then(serde_json::Value::as_str) {
                Some("capability_request") => {
                    capability_requests += 1;
                    if capability_requests > MAX_CAPABILITY_REQUESTS {
                        self.mark_dead();
                        return Err(PluginError::capability(
                            "capability.budget_exceeded",
                            format!(
                                "plugin exceeded the limit of {MAX_CAPABILITY_REQUESTS} capability requests per invocation"
                            ),
                        ));
                    }
                    if let Err(error) = self.handle_capability_request(&frame, deadline) {
                        self.mark_dead();
                        return Err(error);
                    }
                }
                Some("response") => {
                    return response_from_frame(&frame);
                }
                Some("error") => {
                    return Err(PluginError::invoke(
                        frame
                            .get("code")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("plugin_call_failed"),
                        frame
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("worker reported an error"),
                    ))
                }
                _ => {
                    self.mark_dead();
                    return Err(PluginError::invoke(
                        "plugin.crashed",
                        "plugin worker closed or violated the frame protocol",
                    ));
                }
            }
        }
    }

    fn handle_capability_request(
        &mut self,
        frame: &serde_json::Value,
        deadline: Instant,
    ) -> Result<(), PluginError> {
        let id = frame
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                PluginError::wire("worker_frame_invalid", "capability request has no id")
            })?;
        let capability = frame
            .get("capability")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                PluginError::wire("worker_frame_invalid", "capability request has no name")
            })?;
        let request_value = frame.get("request").cloned().unwrap_or_else(|| json!({}));
        let request_bytes = serde_json::to_vec(&request_value)
            .map_err(|error| PluginError::wire("worker_encode_failed", error.to_string()))?;
        if request_bytes.len() > MAX_CAPABILITY_REQUEST_BYTES {
            return Err(PluginError::capability(
                "capability.request_too_large",
                format!(
                    "capability request is {} bytes; limit is {MAX_CAPABILITY_REQUEST_BYTES}",
                    request_bytes.len()
                ),
            ));
        }
        let result = match &mut self.capability_handler {
            Some(handler) => handler(capability, &request_bytes),
            None => Err(PluginError::capability(
                "capability.unavailable",
                "this worker has no host capability bridge",
            )),
        };
        let reply = match result {
            Ok(data) => match String::from_utf8(data) {
                Ok(data) => json!({"kind":"capability_reply","id":id,"data":data}),
                Err(_) => json!({
                    "kind":"capability_error",
                    "id":id,
                    "code":"capability.reply_invalid",
                    "message":"capability reply is not UTF-8 text",
                }),
            },
            Err(error) => json!({
                "kind":"capability_error",
                "id":id,
                "code":error.code,
                "message":error.message,
            }),
        };
        self.connection
            .send(reply, deadline)
            .map_err(invocation_error)
    }

    fn mark_dead(&mut self) {
        self.dead = true;
        self.connection.abort();
    }
}

impl Drop for WorkerPlugin {
    fn drop(&mut self) {
        if !self.dead {
            let _ = self.connection.send(
                json!({"kind": "shutdown"}),
                Instant::now() + Duration::from_millis(500),
            );
        }
        self.connection.abort();
    }
}

fn invocation_error(error: TransportError) -> PluginError {
    match error {
        TransportError::Timeout => PluginError::invoke(
            "plugin.timeout",
            "plugin worker exceeded its invocation timeout",
        ),
        other => transport_error(other),
    }
}

fn transport_error(error: TransportError) -> PluginError {
    match error {
        TransportError::Timeout => {
            PluginError::invoke("plugin.timeout", "plugin worker exceeded its timeout")
        }
        TransportError::Closed => {
            PluginError::invoke("plugin.crashed", "plugin worker closed its output stream")
        }
        TransportError::Frame(FrameError::Io(error)) => PluginError::invoke(
            "plugin.crashed",
            format!("worker frame I/O failed: {error}"),
        ),
        // The shared transport classifies an over-limit or unencodable frame
        // before anything is written. Keep the boundary-specific codes the
        // plugin ABI has always reported instead of flattening them into
        // `worker_frame_invalid`.
        TransportError::Frame(FrameError::Encode(error)) => {
            PluginError::wire("worker_encode_failed", error.to_string())
        }
        TransportError::Frame(FrameError::Length(length)) => PluginError::wire(
            "worker_frame_too_large",
            format!("worker frame is {length} bytes"),
        ),
        TransportError::Frame(other) => {
            PluginError::wire("worker_frame_invalid", other.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_keep_the_boundary_specific_codes() {
        let too_large = transport_error(TransportError::Frame(FrameError::Length(9 * 1024 * 1024)));
        assert_eq!(too_large.code, "worker_frame_too_large");

        let over_limit = transport_error(TransportError::Frame(FrameError::PartialPayload {
            expected: 10,
            actual: 3,
        }));
        assert_eq!(over_limit.code, "worker_frame_invalid");

        let closed = transport_error(TransportError::Closed);
        assert_eq!(closed.code, "plugin.crashed");
    }
}
