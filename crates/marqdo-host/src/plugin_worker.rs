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
    dead: bool,
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
        let mut child = match Command::new(executable)
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
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err(PluginError::load(
                "worker_spawn_failed",
                "worker stdin unavailable",
            ));
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = std::fs::remove_file(&manifest_path);
            return Err(PluginError::load(
                "worker_spawn_failed",
                "worker stdout unavailable",
            ));
        };
        let (sender, frames) = mpsc::channel();
        thread::Builder::new()
            .name("marqdo-plugin-worker-reader".to_string())
            .spawn(move || loop {
                match read_frame(&mut stdout) {
                    Ok(Some(frame)) => {
                        if sender.send(Ok(frame)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = sender.send(Err(PluginError::invoke(
                            "plugin.crashed",
                            "plugin worker closed its output stream",
                        )));
                        break;
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            })
            .map_err(|error| {
                let _ = child.kill();
                PluginError::load(
                    "worker_spawn_failed",
                    format!("cannot start worker reader: {error}"),
                )
            })?;
        let ready = match frames.recv_timeout(HANDSHAKE_TIMEOUT) {
            Ok(frame) => frame,
            Err(_) => {
                let _ = std::fs::remove_file(&manifest_path);
                let _ = child.kill();
                return Err(PluginError::invoke(
                    "plugin.timeout",
                    "plugin worker handshake timed out",
                ));
            }
        };
        let _ = std::fs::remove_file(&manifest_path);
        let ready = ready?;
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
            dead: false,
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
        if let Err(error) = write_frame(
            &mut self.stdin,
            &json!({"kind": "invoke", "request": request_value}),
        ) {
            self.mark_dead();
            return Err(error);
        }
        let frame = match self.frames.recv_timeout(CALL_TIMEOUT) {
            Ok(Ok(frame)) => frame,
            Ok(Err(error)) => {
                self.mark_dead();
                return Err(error);
            }
            Err(_) => {
                self.mark_dead();
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
            _ => {
                self.mark_dead();
                Err(PluginError::invoke(
                    "plugin.crashed",
                    "plugin worker closed or violated the frame protocol",
                ))
            }
        }
    }

    fn mark_dead(&mut self) {
        self.dead = true;
        let _ = self.child.kill();
    }
}

impl Drop for WorkerPlugin {
    fn drop(&mut self) {
        let _ = write_frame(&mut self.stdin, &json!({"kind": "shutdown"}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
