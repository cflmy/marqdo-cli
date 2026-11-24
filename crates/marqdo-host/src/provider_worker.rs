//! Host-managed worker resource provider (ADR-0035).
//!
//! This is a bounded framed-JSON transport for **trusted helper processes**.
//! It is not a C ABI binding and not an OS sandbox: the helper runs as the host
//! user. Registration is a host embedding API; language code only sees the
//! normal `resource.*` operations.
//!
//! Framing, bounded reads, request deadlines and teardown come from
//! [`marqdo_worker_ipc`]. The `open`/`read`/`write`/`close` vocabulary, kind
//! rights and error categories stay owned by the resource boundary, so a
//! shared transport never widens provider authority.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use marqdo_span::Span;
use marqdo_worker_ipc::{Connection, FrameError, TransportError};
use serde_json::{json, Value as Json};

use crate::resource::{ResourceProvider, ResourceRead};
use marqdo_value::{ErrorCategory, RuntimeError};

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);

/// A `ResourceProvider` backed by a child process speaking framed JSON.
pub(crate) struct WorkerResourceProvider {
    name: String,
    kinds: Vec<String>,
    connection: Connection,
}

impl WorkerResourceProvider {
    pub(crate) fn spawn(
        name: &str,
        kinds: &[String],
        program: &Path,
        args: &[String],
    ) -> Result<Self, RuntimeError> {
        let child = Command::new(program)
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
        let connection = Connection::attach(child, MAX_FRAME_BYTES)
            .map_err(|error| worker_error(&format!("cannot attach to worker: {error}")))?;
        let mut provider = Self {
            name: name.to_string(),
            kinds: kinds.to_vec(),
            connection,
        };
        // Handshake: the helper must answer a ping before it is trusted.
        provider.request(json!({"op": "ping"}))?;
        Ok(provider)
    }

    fn request(&mut self, request: Json) -> Result<Json, RuntimeError> {
        let deadline = Instant::now() + WORKER_TIMEOUT;
        self.connection
            .send(request, deadline)
            .map_err(transport_error)?;
        let response = self.connection.receive(deadline).map_err(transport_error)?;
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
        let _ = self.connection.send(
            json!({"op": "shutdown"}),
            Instant::now() + Duration::from_millis(500),
        );
        self.connection.abort();
    }
}

fn with_span(mut error: RuntimeError, span: Option<Span>) -> RuntimeError {
    if error.span.is_none() {
        error.span = span;
    }
    error
}

fn transport_error(error: TransportError) -> RuntimeError {
    match error {
        TransportError::Timeout => {
            worker_error("worker resource provider did not answer before the deadline")
        }
        TransportError::Closed => worker_error("worker resource provider closed its transport"),
        TransportError::Frame(FrameError::Io(error)) => {
            worker_error(&format!("worker resource provider I/O failed: {error}"))
        }
        TransportError::Frame(other) => {
            worker_error(&format!("worker resource provider frame error: {other}"))
        }
    }
}

fn worker_error(message: &str) -> RuntimeError {
    RuntimeError::new(ErrorCategory::HostError, None, message.to_string())
}
