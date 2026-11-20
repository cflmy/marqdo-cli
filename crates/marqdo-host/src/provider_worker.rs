//! Host-managed worker resource provider (ADR-0035).
//!
//! This is a small, versioned framed-JSON transport for **trusted helper
//! processes**. It is not a C ABI binding and not an OS sandbox: the helper
//! runs as the host user. Registration is a host embedding API; language code
//! only sees the normal `resource.*` operations.

use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use marqdo_span::Span;
use serde_json::{json, Value as Json};

use crate::resource::{ResourceProvider, ResourceRead};
use marqdo_value::{ErrorCategory, RuntimeError};

const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// A `ResourceProvider` backed by a child process speaking framed JSON.
pub(crate) struct WorkerResourceProvider {
    name: String,
    kinds: Vec<String>,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WorkerResourceProvider {
    pub(crate) fn spawn(
        name: &str,
        kinds: &[String],
        program: &Path,
        args: &[String],
    ) -> Result<Self, RuntimeError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::KernelUnavailable,
                    None,
                    format!(
                        "cannot spawn worker resource provider `{name}` ({}): {error}",
                        program.display()
                    ),
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| worker_error("missing stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| worker_error("missing stdout pipe"))?;
        let mut provider = Self {
            name: name.to_string(),
            kinds: kinds.to_vec(),
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };
        // Handshake: the helper must answer a ping before it is trusted.
        provider.request(json!({"op": "ping"}))?;
        Ok(provider)
    }

    fn request(&mut self, request: Json) -> Result<Json, RuntimeError> {
        let bytes = serde_json::to_vec(&request)
            .map_err(|error| worker_error(&format!("cannot encode request: {error}")))?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(worker_error("request frame is too large"));
        }
        self.stdin
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .and_then(|()| self.stdin.write_all(&bytes))
            .and_then(|()| self.stdin.flush())
            .map_err(|error| worker_error(&format!("cannot write request frame: {error}")))?;
        let mut header = [0u8; 4];
        self.stdout
            .read_exact(&mut header)
            .map_err(|error| worker_error(&format!("cannot read response header: {error}")))?;
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(worker_error("invalid response frame length"));
        }
        let mut payload = vec![0u8; length];
        self.stdout
            .read_exact(&mut payload)
            .map_err(|error| worker_error(&format!("cannot read response frame: {error}")))?;
        let response: Json = serde_json::from_slice(&payload)
            .map_err(|error| worker_error(&format!("invalid response JSON: {error}")))?;
        if response.get("ok").and_then(Json::as_bool) != Some(true) {
            let code = response
                .get("code")
                .and_then(Json::as_str)
                .unwrap_or("host.error");
            let message = response
                .get("message")
                .and_then(Json::as_str)
                .unwrap_or("worker resource provider failed");
            let category = match code {
                "permission.denied" => ErrorCategory::PermissionDenied,
                "resource.closed" => ErrorCategory::ResourceClosed,
                "resource.exhausted" => ErrorCategory::ResourceExhausted,
                "resource.unsupported" => ErrorCategory::ResourceUnsupported,
                _ => ErrorCategory::HostError,
            };
            return Err(RuntimeError::new(category, None, message.to_string()));
        }
        Ok(response)
    }
}

impl ResourceProvider for WorkerResourceProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn kinds(&self) -> &[&str] {
        // Owned kinds are exposed through `kind_strings`; registration never
        // calls this for the worker transport.
        &[]
    }
    fn kind_strings(&self) -> Vec<String> {
        self.kinds.clone()
    }
    fn open(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        span: Option<Span>,
    ) -> Result<u64, RuntimeError> {
        let response = self
            .request(json!({
                "op": "open",
                "kind": kind,
                "descriptor": descriptor,
                "rights": rights,
            }))
            .map_err(|error| with_span(error, span))?;
        response
            .get("token")
            .and_then(Json::as_u64)
            .ok_or_else(|| worker_error("open response has no token"))
    }
    fn read(
        &mut self,
        token: u64,
        max_bytes: usize,
        span: Option<Span>,
    ) -> Result<ResourceRead, RuntimeError> {
        let response = self
            .request(json!({"op": "read", "token": token, "max_bytes": max_bytes}))
            .map_err(|error| with_span(error, span))?;
        let bytes = response
            .get("bytes")
            .and_then(Json::as_array)
            .ok_or_else(|| worker_error("read response has no bytes"))?
            .iter()
            .map(|value| value.as_u64().and_then(|v| u8::try_from(v).ok()))
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(|| worker_error("read response contains a non-byte value"))?;
        let eof = response.get("eof").and_then(Json::as_bool).unwrap_or(false);
        Ok(ResourceRead { bytes, eof })
    }
    fn write(
        &mut self,
        token: u64,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<usize, RuntimeError> {
        let response = self
            .request(json!({"op": "write", "token": token, "bytes": bytes}))
            .map_err(|error| with_span(error, span))?;
        response
            .get("count")
            .and_then(Json::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| worker_error("write response has no count"))
    }
    fn close(&mut self, token: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        self.request(json!({"op": "close", "token": token}))
            .map_err(|error| with_span(error, span))?;
        Ok(())
    }
}

impl Drop for WorkerResourceProvider {
    fn drop(&mut self) {
        let _ = self.request(json!({"op": "shutdown"}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn with_span(mut error: RuntimeError, span: Option<Span>) -> RuntimeError {
    if error.span.is_none() {
        error.span = span;
    }
    error
}

fn worker_error(message: &str) -> RuntimeError {
    RuntimeError::new(ErrorCategory::HostError, None, message.to_string())
}
