//! Minimal third-party plugin built only against the public Rust SDK.
//!
//! It exercises the typed wire helpers (`decode_request`, `int_arg`,
//! `success`) and a `Value::Int` result. Real negotiation, loading and
//! invocation are performed by the frozen CLI in
//! `tools/ci/check_plugin_sdk.py`.

use marqdo_plugin_sdk::wire;
use marqdo_plugin_sdk::{mq_plugin_entry, CallContext, PluginError, Value};

fn add(_ctx: &mut CallContext, args: &[u8]) -> Result<Vec<u8>, PluginError> {
    let request = wire::decode_request(args)?;
    let values = request.typed_args()?;
    let a = wire::int_arg(&values, "a")?;
    let b = wire::int_arg(&values, "b")?;
    wire::success(Some(Value::Int((a + b).into())))
}

fn echo_text(_ctx: &mut CallContext, args: &[u8]) -> Result<Vec<u8>, PluginError> {
    let request = wire::decode_request(args)?;
    let values = request.typed_args()?;
    let text = wire::text_arg(&values, "text")?;
    wire::success(Some(Value::text(text)))
}

mq_plugin_entry! {
    name: "sdk.demo",
    version: "0.1.0",
    exports: {
        "sdk.add" => "($a: Int, $b: Int) -> Int" => add,
        "sdk.echo" => "($text: Text) -> Text" => echo_text,
    }
}
