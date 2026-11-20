//! Host side of the isolated plugin worker transport (ADR-0010).
//!
//! The worker is a child process running the same core binary with the hidden
//! `__plugin-worker` command. It isolates crashes and keeps native code out of
//! the interpreter process. A plain process is **not** an OS sandbox; strict
//! mode still refuses without a real isolation adapter.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use marqdo_plugin_abi::worker::{read_frame, write_frame, WORKER_COMMAND};
use marqdo_plugin_abi::{PluginError, PluginManifest, WireRequest, WireResponse};
use serde_json::json;

/// A native plugin running in an isolated worker process.
pub struct WorkerPlugin {
    child: Child,
    stdin: ChildStdin,
    frames: Receiver<Result<serde_json::Value, PluginError>>,
    exports: BTreeMap<String, String>,
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

impl std::fmt::Debug for WorkerPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerPlugin")
            .field("pid", &self.child.id())
            .field("exports", &self.exports.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl WorkerPlugin {
    /// Spawn a worker for `manifest` and wait for its ready frame.
    pub fn spawn(manifest: &PluginManifest, base_dir: &Path) -> Result<Self, PluginError> {
        let manifest_path = if base_dir.join("plugin.yaml").is_file() {
            base_dir.join("plugin.yaml")
        } else {
            return Err(PluginError::load(
                "worker_manifest_missing",
                format!("no plugin.yaml in `{}`", base_dir.display()),
            ));
        };
        let executable = std::env::current_exe().map_err(|error| {
            PluginError::load(
                "worker_spawn_failed",
                format!("cannot find the current executable: {error}"),
            )
        })?;
        let mut child = Command::new(executable)
            .arg(WORKER_COMMAND)
            .arg(&manifest_path)
            .arg(base_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                PluginError::load(
                    "worker_spawn_failed",
                    format!("cannot start plugin worker: {error}"),
                )
            })?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err(PluginError::load(
                "worker_spawn_failed",
                "worker stdin unavailable",
            ));
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(PluginError::load(
                "worker_spawn_failed",
                "worker stdout unavailable",
            ));
        };
        let (sender, frames) = mpsc::channel();
        thread::Builder::new()
            .name("marqdo-plugin-worker-reader".to_string())
            .spawn(move || loop {
                let frame = read_frame(&mut stdout);
                let failed = frame.is_err();
                if sender.send(frame).is_err() || failed {
                    break;
                }
            })
            .map_err(|error| {
                let _ = child.kill();
                PluginError::load(
                    "worker_spawn_failed",
                    format!("cannot start worker reader: {error}"),
                )
            })?;
        let ready = frames.recv_timeout(HANDSHAKE_TIMEOUT).map_err(|_| {
            let _ = child.kill();
            PluginError::invoke("plugin.timeout", "plugin worker handshake timed out")
        })??;
        if ready.get("kind").and_then(serde_json::Value::as_str) != Some("ready") {
            let _ = child.kill();
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
            let _ = child.kill();
            return Err(PluginError::manifest(
                "worker_export_mismatch",
                "worker exports do not match the manifest",
            ));
        }
        Ok(Self {
            child,
            stdin,
            frames,
            exports,
        })
    }

    /// Invoke one export in the worker.
    pub fn invoke(&mut self, request: &WireRequest) -> Result<WireResponse, PluginError> {
        let request_value: serde_json::Value = serde_json::from_slice(&request.to_json_bytes()?)
            .map_err(|error| PluginError::wire("worker_encode_failed", error.to_string()))?;
        write_frame(
            &mut self.stdin,
            &json!({"kind": "invoke", "request": request_value}),
        )?;
        let frame = match self.frames.recv_timeout(CALL_TIMEOUT) {
            Ok(frame) => frame?,
            Err(_) => {
                let _ = self.child.kill();
                return Err(PluginError::invoke(
                    "plugin.timeout",
                    "plugin worker exceeded its invocation timeout",
                ));
            }
        };
        match frame.get("kind").and_then(serde_json::Value::as_str) {
            Some("response") => marqdo_plugin_abi::worker::response_from_frame(&frame),
            Some("error") => Err(PluginError::invoke(
                frame
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("plugin_call_failed"),
                frame
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("worker reported an error"),
            )),
            _ => Err(PluginError::invoke(
                "plugin.crashed",
                "plugin worker closed or violated the frame protocol",
            )),
        }
    }
}

impl Drop for WorkerPlugin {
    fn drop(&mut self) {
        let _ = write_frame(&mut self.stdin, &json!({"kind": "shutdown"}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
