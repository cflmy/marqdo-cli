//! `wire_version` 1 JSON request/response envelope used across the plugin ABI
//! and (logically identical) worker IPC.
//!
//! The envelope is intentionally small; the actual values inside `args` and
//! `value` use the canonical type-tagged representation from
//! [`marqdo_value::wire`] (`spec/value-wire.md`): integers are decimal
//! strings, special floats carry explicit tags, and every value is a JSON
//! object with a `type` field.
//!
//! ```json
//! {
//!   "wire_version": 1,
//!   "kind": "request",
//!   "function": "solver.solve",
//!   "args": {"a": {"type": "int", "decimal": "12345678901234567890"}},
//!   "context": {"trace_id": "..."}
//! }
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use marqdo_value::wire::{from_wire_with_limits, to_wire, WireLimits};
use marqdo_value::Value;

use crate::abi::{MAX_ERROR_WIRE_BYTES, MAX_REQUEST_WIRE_BYTES, MAX_RESULT_WIRE_BYTES};
use crate::error::{ErrorKind, PluginError};

/// Wire protocol version (`wire_version` field).
pub const WIRE_VERSION: u32 = 1;
/// Envelope kind for requests.
pub const KIND_REQUEST: &str = "request";
/// Envelope kind for responses (success and error).
pub const KIND_RESPONSE: &str = "response";
/// Maximum number of request arguments.
pub const MAX_ARGS: usize = 1024;

/// Per-call context attached to a request. Contains no host secrets and no
/// reconstructable OS handles: only opaque, scoped identifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireContext {
    /// Opaque trace identifier for host logging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Opaque capability handle for this call; never a real OS handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_handle: Option<String>,
}

/// A `kind: "request"` envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireRequest {
    /// Wire protocol version; must be [`WIRE_VERSION`].
    pub wire_version: u32,
    /// Envelope kind; must be [`KIND_REQUEST`].
    pub kind: String,
    /// Fully qualified exported function name (e.g. `solver.add`).
    pub function: String,
    /// Type-tagged argument values keyed by parameter name.
    #[serde(default)]
    pub args: BTreeMap<String, Json>,
    /// Per-call context.
    #[serde(default)]
    pub context: WireContext,
}

impl WireRequest {
    /// Start a request for `function`.
    pub fn new(function: impl Into<String>) -> Self {
        Self {
            wire_version: WIRE_VERSION,
            kind: KIND_REQUEST.to_string(),
            function: function.into(),
            args: BTreeMap::new(),
            context: WireContext::default(),
        }
    }

    /// Add a typed argument, encoding it with the canonical value wire tags.
    #[must_use]
    pub fn with_arg(mut self, name: impl Into<String>, value: &Value) -> Self {
        self.args.insert(name.into(), to_wire(value));
        self
    }

    /// Add an already-encoded type-tagged argument.
    #[must_use]
    pub fn with_raw_arg(mut self, name: impl Into<String>, value: Json) -> Self {
        self.args.insert(name.into(), value);
        self
    }

    /// Attach a trace identifier.
    #[must_use]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.context.trace_id = Some(trace_id.into());
        self
    }

    /// Decode all arguments into typed [`Value`]s, enforcing wire limits.
    pub fn typed_args(&self) -> Result<BTreeMap<String, Value>, PluginError> {
        self.typed_args_with_limits(WireLimits::default())
    }

    /// Decode all arguments into typed [`Value`]s with explicit limits.
    pub fn typed_args_with_limits(
        &self,
        limits: WireLimits,
    ) -> Result<BTreeMap<String, Value>, PluginError> {
        let mut decoded = BTreeMap::new();
        for (name, json) in &self.args {
            let value = from_wire_with_limits(json, limits).map_err(|error| {
                PluginError::wire(
                    "invalid_argument_wire",
                    format!("argument `{name}` is not a valid typed wire value"),
                )
                .with_detail(error.to_string())
            })?;
            decoded.insert(name.clone(), value);
        }
        Ok(decoded)
    }

    /// Encode the request as UTF-8 JSON bytes, enforcing the request size cap.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, PluginError> {
        let bytes = serde_json::to_vec(self).map_err(|error| {
            PluginError::wire("request_encode_failed", "could not encode wire request")
                .with_detail(error.to_string())
        })?;
        if bytes.len() > MAX_REQUEST_WIRE_BYTES {
            return Err(PluginError::wire(
                "request_too_large",
                format!(
                    "request wire is {} bytes, limit is {MAX_REQUEST_WIRE_BYTES}",
                    bytes.len()
                ),
            ));
        }
        Ok(bytes)
    }

    /// Decode and validate a request envelope.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.len() > MAX_REQUEST_WIRE_BYTES {
            return Err(PluginError::wire(
                "request_too_large",
                format!(
                    "request wire is {} bytes, limit is {MAX_REQUEST_WIRE_BYTES}",
                    bytes.len()
                ),
            ));
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|error| {
            PluginError::wire("request_invalid", "wire request is not valid JSON")
                .with_detail(error.to_string())
        })?;
        request.validate()?;
        Ok(request)
    }

    /// Validate envelope invariants without touching the argument values.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.wire_version != WIRE_VERSION {
            return Err(PluginError::wire(
                "unsupported_wire_version",
                format!(
                    "wire_version {} is not supported (host supports {WIRE_VERSION})",
                    self.wire_version
                ),
            ));
        }
        if self.kind != KIND_REQUEST {
            return Err(PluginError::wire(
                "unexpected_envelope_kind",
                format!("expected kind `{KIND_REQUEST}`, got `{}`", self.kind),
            ));
        }
        if self.function.trim().is_empty() {
            return Err(PluginError::wire(
                "missing_function",
                "wire request has an empty function name",
            ));
        }
        if self.args.len() > MAX_ARGS {
            return Err(PluginError::wire(
                "too_many_arguments",
                format!("wire request has more than {MAX_ARGS} arguments"),
            ));
        }
        Ok(())
    }
}

/// Structured error body inside an error response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireErrorBody {
    /// Error category (for example `invoke`, `capability`).
    pub kind: String,
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable message (never contains secrets).
    pub message: String,
    /// Whether the caller may retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
    /// Optional non-sensitive detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl WireErrorBody {
    /// Build a wire error body from a structured plugin error.
    pub fn from_plugin_error(error: &PluginError) -> Self {
        Self {
            kind: error.kind.as_str().to_string(),
            code: error.code.clone(),
            message: error.message.clone(),
            recoverable: error.recoverable.then_some(true),
            detail: error.detail.clone(),
        }
    }

    /// Convert back into a structured plugin error.
    pub fn to_plugin_error(&self) -> PluginError {
        PluginError {
            kind: ErrorKind::from_wire_name(&self.kind).unwrap_or(ErrorKind::Invoke),
            code: self.code.clone(),
            message: self.message.clone(),
            detail: self.detail.clone(),
            recoverable: self.recoverable.unwrap_or(false),
        }
    }
}

/// A `kind: "response"` envelope: either `value` or `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireResponse {
    /// Wire protocol version; must be [`WIRE_VERSION`].
    pub wire_version: u32,
    /// Envelope kind; must be [`KIND_RESPONSE`].
    pub kind: String,
    /// Type-tagged result value, absent for unit results and errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Json>,
    /// Structured error, absent for successful responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WireErrorBody>,
}

impl WireResponse {
    /// Successful response carrying an optional typed value.
    pub fn success(value: Option<Value>) -> Self {
        Self {
            wire_version: WIRE_VERSION,
            kind: KIND_RESPONSE.to_string(),
            value: value.as_ref().map(to_wire),
            error: None,
        }
    }

    /// Successful response carrying an already-encoded type-tagged value.
    pub fn success_raw(value: Option<Json>) -> Self {
        Self {
            wire_version: WIRE_VERSION,
            kind: KIND_RESPONSE.to_string(),
            value,
            error: None,
        }
    }

    /// Error response.
    pub fn from_error(error: &PluginError) -> Self {
        Self {
            wire_version: WIRE_VERSION,
            kind: KIND_RESPONSE.to_string(),
            value: None,
            error: Some(WireErrorBody::from_plugin_error(error)),
        }
    }

    /// Whether this response is an error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Encode as UTF-8 JSON bytes, enforcing the result size cap.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, PluginError> {
        let bytes = serde_json::to_vec(self).map_err(|error| {
            PluginError::wire("response_encode_failed", "could not encode wire response")
                .with_detail(error.to_string())
        })?;
        if bytes.len() > MAX_RESULT_WIRE_BYTES {
            return Err(PluginError::wire(
                "response_too_large",
                format!(
                    "response wire is {} bytes, limit is {MAX_RESULT_WIRE_BYTES}",
                    bytes.len()
                ),
            ));
        }
        Ok(bytes)
    }

    /// Decode and validate a response envelope.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, PluginError> {
        Self::decode(bytes, MAX_RESULT_WIRE_BYTES, "response")
    }

    /// Decode and validate an error envelope using the tighter error cap.
    pub fn from_error_json_bytes(bytes: &[u8]) -> Result<Self, PluginError> {
        Self::decode(bytes, MAX_ERROR_WIRE_BYTES, "error")
    }

    fn decode(bytes: &[u8], limit: usize, what: &str) -> Result<Self, PluginError> {
        if bytes.len() > limit {
            return Err(PluginError::wire(
                "response_too_large",
                format!("{what} wire is {} bytes, limit is {limit}", bytes.len()),
            ));
        }
        let response: Self = serde_json::from_slice(bytes).map_err(|error| {
            PluginError::wire("response_invalid", format!("wire {what} is not valid JSON"))
                .with_detail(error.to_string())
        })?;
        if response.wire_version != WIRE_VERSION {
            return Err(PluginError::wire(
                "unsupported_wire_version",
                format!(
                    "wire_version {} is not supported (host supports {WIRE_VERSION})",
                    response.wire_version
                ),
            ));
        }
        if response.kind != KIND_RESPONSE {
            return Err(PluginError::wire(
                "unexpected_envelope_kind",
                format!("expected kind `{KIND_RESPONSE}`, got `{}`", response.kind),
            ));
        }
        match (&response.value, &response.error) {
            (Some(_), Some(_)) => Err(PluginError::wire(
                "ambiguous_response",
                "wire response must not contain both `value` and `error`",
            )),
            // A response with neither field is a successful no-value (unit)
            // result; callers see `typed_value() == None`.
            _ => Ok(response),
        }
    }

    /// Convert an error response into a structured error.
    pub fn into_result(self) -> Result<Self, PluginError> {
        match self.error {
            Some(error) => Err(error.to_plugin_error()),
            None => Ok(self),
        }
    }

    /// The successful value decoded into a typed [`Value`].
    pub fn typed_value(&self) -> Result<Option<Value>, PluginError> {
        match &self.value {
            Some(json) => from_wire_with_limits(json, WireLimits::default())
                .map(Some)
                .map_err(|error| {
                    PluginError::wire(
                        "invalid_result_wire",
                        "response value is not a valid typed wire value",
                    )
                    .with_detail(error.to_string())
                }),
            None => Ok(None),
        }
    }
}

/// Encode a typed [`Value`] into its canonical type-tagged JSON form.
pub fn to_wire_json(value: &Value) -> Json {
    to_wire(value)
}

/// Decode an `i64` from a typed wire value (returns `None` for any other type
/// or for integers outside the `i64` range).
pub fn int_from_value(value: &Value) -> Option<i64> {
    match value {
        Value::Int(integer) => integer.to_string().parse::<i64>().ok(),
        _ => None,
    }
}

/// Borrow the text from a typed wire value.
pub fn text_from_value(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text),
        _ => None,
    }
}

/// Borrow the boolean from a typed wire value.
pub fn bool_from_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_preserves_type_tags() {
        let request = WireRequest::new("solver.add")
            .with_arg("a", &Value::Int(i64::MAX.into()))
            .with_arg("b", &Value::Int(2_i64.into()))
            .with_trace_id("trace-1");
        let bytes = request.to_json_bytes().expect("encode");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(text.contains("\"type\":\"int\""));
        assert!(text.contains(&format!("\"decimal\":\"{}\"", i64::MAX)));
        let decoded = WireRequest::from_json_bytes(&bytes).expect("decode");
        assert_eq!(decoded, request);
        let args = decoded.typed_args().expect("typed args");
        assert_eq!(int_from_value(&args["a"]), Some(i64::MAX));

        // Integers beyond i64 stay exact on the wire but are not coercible.
        let big = WireRequest::new("solver.add").with_raw_arg(
            "big",
            serde_json::json!({"type": "int", "decimal": "123456789012345678901234567890"}),
        );
        let values = big.typed_args().expect("typed big");
        assert_eq!(int_from_value(&values["big"]), None);
    }

    #[test]
    fn request_rejects_wrong_version_or_kind() {
        let mut request = WireRequest::new("solver.add");
        request.wire_version = 2;
        assert_eq!(
            WireRequest::from_json_bytes(&serde_json::to_vec(&request).expect("json"))
                .expect_err("version")
                .code,
            "unsupported_wire_version"
        );
        let mut request = WireRequest::new("solver.add");
        request.kind = "response".to_string();
        assert_eq!(
            WireRequest::from_json_bytes(&serde_json::to_vec(&request).expect("json"))
                .expect_err("kind")
                .code,
            "unexpected_envelope_kind"
        );
    }

    #[test]
    fn response_success_and_error_roundtrip() {
        let success = WireResponse::success(Some(Value::Int(42_i64.into())));
        let decoded = WireResponse::from_json_bytes(&success.to_json_bytes().expect("encode"))
            .expect("decode");
        assert_eq!(
            decoded.typed_value().expect("typed"),
            Some(Value::Int(42_i64.into()))
        );

        let error = PluginError::invoke("bad_arguments", "argument `a` is missing");
        let response = WireResponse::from_error(&error);
        let bytes = response.to_json_bytes().expect("encode");
        assert!(String::from_utf8_lossy(&bytes).contains("\"error\""));
        let decoded = WireResponse::from_error_json_bytes(&bytes).expect("decode");
        assert_eq!(decoded.into_result().expect_err("error"), error);
    }

    #[test]
    fn response_rejects_ambiguous_payloads_and_accepts_unit() {
        let ambiguous = serde_json::json!({
            "wire_version": 1,
            "kind": "response",
            "value": {"type": "int", "decimal": "1"},
            "error": {"kind": "invoke", "code": "x", "message": "y"}
        });
        assert_eq!(
            WireResponse::from_json_bytes(&serde_json::to_vec(&ambiguous).expect("json"))
                .expect_err("ambiguous")
                .code,
            "ambiguous_response"
        );
        // A response without `value` or `error` is a successful unit result.
        let unit = serde_json::json!({"wire_version": 1, "kind": "response"});
        let decoded = WireResponse::from_json_bytes(&serde_json::to_vec(&unit).expect("json"))
            .expect("unit response");
        assert!(!decoded.is_error());
        assert_eq!(decoded.typed_value().expect("typed"), None);
        assert!(decoded.into_result().is_ok());
    }

    #[test]
    fn oversized_wire_is_rejected_before_json_parsing() {
        let oversized = vec![b' '; MAX_REQUEST_WIRE_BYTES + 1];
        assert_eq!(
            WireRequest::from_json_bytes(&oversized)
                .expect_err("oversized")
                .code,
            "request_too_large"
        );
    }
}
