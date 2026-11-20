//! Per-invocation context passed to plugin handlers.
//!
//! The context is valid only for the duration of one invocation. It exists to
//! carry host capabilities into the plugin: capability requests are answered
//! by the host's authorization layer and can never be escalated by plugin
//! code. Do not cache this value, its raw pointer or any capability token.

use marqdo_plugin_abi::abi::{
    MqBytes, MqCallContext, MqHostApi, MqOwnedBytes, MAX_CAPABILITY_REPLY_BYTES,
};
use marqdo_plugin_abi::PluginError;

use crate::bytes;

/// Context for one plugin invocation.
pub struct CallContext {
    raw: *mut MqCallContext,
    host: &'static MqHostApi,
}

impl CallContext {
    /// Wrap the host-provided context pointer.
    ///
    /// # Safety
    ///
    /// `raw` must be the context pointer provided by the host for the current
    /// invocation, and `host` must be the validated host API table.
    pub(crate) unsafe fn new(raw: *mut MqCallContext, host: &'static MqHostApi) -> Self {
        Self { raw, host }
    }

    /// The host API table of this plugin instance.
    pub fn host(&self) -> &'static MqHostApi {
        self.host
    }

    /// Request an already-authorized host capability for this call.
    ///
    /// `request` is a UTF-8 JSON capability request; the reply is opaque bytes
    /// owned by the host. The SDK copies the reply and releases the host buffer.
    pub fn capability_request(
        &mut self,
        name: &str,
        request: &[u8],
    ) -> Result<Vec<u8>, PluginError> {
        if self.raw.is_null() {
            return Err(PluginError::capability(
                "missing_call_context",
                "capability request requires an active invocation context",
            ));
        }
        let mut reply = MqOwnedBytes::EMPTY;
        // SAFETY: raw context and host table are valid for this invocation.
        let code = unsafe {
            (self.host.request_capability)(
                self.raw,
                MqBytes::from_slice(name.as_bytes()),
                MqBytes::from_slice(request),
                &mut reply,
            )
        };
        // SAFETY: host-allocated reply is initialized for `reply.len` bytes.
        let reply_bytes = unsafe { reply.to_vec() };
        // SAFETY: the host allocated `reply` with its allocator for us; release
        // it exactly once after copying.
        unsafe { bytes::release_host_buffer(self.host, reply) };
        let reply_bytes = reply_bytes.map_err(|error| {
            PluginError::capability(
                "invalid_capability_reply",
                "host capability reply violates the buffer contract",
            )
            .with_detail(error.to_string())
        })?;
        if reply_bytes.len() > MAX_CAPABILITY_REPLY_BYTES {
            return Err(PluginError::capability(
                "capability_reply_too_large",
                format!(
                    "capability reply is {} bytes, limit is {MAX_CAPABILITY_REPLY_BYTES}",
                    reply_bytes.len()
                ),
            ));
        }
        if code != crate::result_code::OK {
            return Err(PluginError::capability(
                "capability_request_failed",
                format!("host rejected capability request `{name}`"),
            )
            .with_detail(format!(
                "result code `{}` ({code})",
                crate::result_code::name(code)
            )));
        }
        Ok(reply_bytes)
    }

    /// Best-effort structured log through the `log` capability.
    pub fn log(&mut self, level: &str, message: &str) -> Result<(), PluginError> {
        let request = serde_json::json!({"level": level, "message": message});
        let request = serde_json::to_vec(&request).map_err(|error| {
            PluginError::capability("log_encode_failed", "could not encode log request")
                .with_detail(error.to_string())
        })?;
        let _ = self.capability_request("log", &request)?;
        Ok(())
    }
}

impl std::fmt::Debug for CallContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallContext")
            .field("raw", &self.raw)
            .field("host", &(self.host as *const MqHostApi))
            .finish()
    }
}
