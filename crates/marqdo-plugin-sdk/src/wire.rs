//! Wire envelope helpers for plugin handlers.
//!
//! The canonical envelope is defined by `marqdo-plugin-abi` and documented in
//! `spec/value-wire.md`: UTF-8 JSON, `wire_version == 1`, type-tagged values.
//! Handlers normally do:
//!
//! ```ignore
//! fn add(ctx: &mut CallContext, args: &[u8]) -> Result<Vec<u8>, PluginError> {
//!     let request = decode_request(args)?;
//!     let values = request.typed_args()?;
//!     success(Some(Value::Int((int_arg(&values, "a")? + int_arg(&values, "b")?).into())))
//! }
//! ```

use std::collections::BTreeMap;

use marqdo_plugin_abi::{
    bool_from_value, int_from_value, text_from_value, PluginError, Value, WireRequest, WireResponse,
};
use serde_json::Value as Json;

/// Decode and validate a request envelope received from the host.
pub fn decode_request(bytes: &[u8]) -> Result<WireRequest, PluginError> {
    WireRequest::from_json_bytes(bytes)
}

/// Look up a typed argument by name.
pub fn value_arg<'a>(
    values: &'a BTreeMap<String, Value>,
    name: &str,
) -> Result<&'a Value, PluginError> {
    values.get(name).ok_or_else(|| {
        PluginError::invoke(
            "missing_argument",
            format!("required argument `{name}` is missing"),
        )
    })
}

/// Look up an integer argument (must fit in `i64`).
pub fn int_arg(values: &BTreeMap<String, Value>, name: &str) -> Result<i64, PluginError> {
    let value = value_arg(values, name)?;
    int_from_value(value).ok_or_else(|| {
        PluginError::invoke(
            "invalid_argument_type",
            format!("argument `{name}` must be an Int"),
        )
    })
}

/// Look up a text argument.
pub fn text_arg<'a>(
    values: &'a BTreeMap<String, Value>,
    name: &str,
) -> Result<&'a str, PluginError> {
    let value = value_arg(values, name)?;
    text_from_value(value).ok_or_else(|| {
        PluginError::invoke(
            "invalid_argument_type",
            format!("argument `{name}` must be Text"),
        )
    })
}

/// Look up a boolean argument.
pub fn bool_arg(values: &BTreeMap<String, Value>, name: &str) -> Result<bool, PluginError> {
    let value = value_arg(values, name)?;
    bool_from_value(value).ok_or_else(|| {
        PluginError::invoke(
            "invalid_argument_type",
            format!("argument `{name}` must be Bool"),
        )
    })
}

/// Encode a successful response envelope.
pub fn success(value: Option<Value>) -> Result<Vec<u8>, PluginError> {
    success_raw(value.as_ref().map(marqdo_plugin_abi::to_wire_json))
}

/// Encode a successful response with an already-encoded type-tagged value.
pub fn success_raw(value: Option<Json>) -> Result<Vec<u8>, PluginError> {
    WireResponse::success_raw(value).to_json_bytes()
}

/// Encode an error response envelope.
///
/// The SDK's dispatch layer uses this automatically when a handler returns an
/// error; handlers may also build custom error responses with it.
pub fn error_response(error: &PluginError) -> Result<Vec<u8>, PluginError> {
    WireResponse::from_error(error).to_json_bytes()
}

/// Decode a response value back into a typed [`Value`] (useful in tests and
/// plugins that compose calls).
pub fn response_value(bytes: &[u8]) -> Result<Option<Value>, PluginError> {
    WireResponse::from_json_bytes(bytes)?.typed_value()
}
