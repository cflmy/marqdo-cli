//! Versioned, type-tagged JSON wire format for values crossing ABI, worker IPC
//! and foreign-kernel boundaries.
//!
//! JSON numbers cannot represent arbitrary precision integers or special
//! floats faithfully, so every value carries an explicit `type` tag and
//! integers are encoded as decimal strings. This is the single canonical
//! exchange format required by the specification (`spec/value-wire.md`).

use std::collections::BTreeMap;

use num_bigint::BigInt;
use serde_json::{json, Map, Value as Json};

use crate::value::{CodeValue, FormulaValue, FunctionRef, PluginHandle, Table, Value};
use marqdo_span::Span;

/// Current wire protocol version.
pub const WIRE_VERSION: u32 = 1;

/// Limits applied while decoding untrusted wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireLimits {
    /// Maximum nesting depth.
    pub max_depth: usize,
    /// Maximum number of elements in a list.
    pub max_items: usize,
    /// Maximum number of bytes in a single string.
    pub max_string_bytes: usize,
}

impl Default for WireLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_items: 1_000_000,
            max_string_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Failure while decoding or encoding wire data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// The `type` tag is missing or unknown.
    UnknownType(String),
    /// A required field is missing.
    MissingField(&'static str),
    /// A field has the wrong JSON type or an invalid value.
    Invalid(String),
    /// Nesting or size limits were exceeded.
    LimitExceeded(&'static str),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::UnknownType(t) => write!(f, "unknown wire value type `{t}`"),
            WireError::MissingField(field) => write!(f, "missing wire field `{field}`"),
            WireError::Invalid(msg) => write!(f, "invalid wire value: {msg}"),
            WireError::LimitExceeded(what) => write!(f, "wire limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for WireError {}

/// Encode a value as the canonical wire representation.
pub fn to_wire(value: &Value) -> Json {
    encode(value)
}

fn encode(value: &Value) -> Json {
    match value {
        Value::Unit => json!({"type": "unit"}),
        Value::None => json!({"type": "none"}),
        Value::Bool(b) => json!({"type": "bool", "value": b}),
        Value::Int(i) => json!({"type": "int", "decimal": i.to_string()}),
        Value::Float(f) => {
            if f.is_nan() {
                json!({"type": "float", "special": "nan"})
            } else if f.is_infinite() {
                json!({"type": "float", "special": if f.is_sign_negative() { "-infinity" } else { "infinity" }})
            } else {
                json!({"type": "float", "value": f})
            }
        }
        Value::Text(t) => json!({"type": "text", "value": t}),
        Value::List(items) => json!({
            "type": "list",
            "items": items.iter().map(encode).collect::<Vec<_>>(),
        }),
        Value::Map(entries) => {
            let mut map = Map::new();
            for (key, val) in entries {
                map.insert(key.clone(), encode(val));
            }
            json!({"type": "map", "entries": Json::Object(map)})
        }
        Value::Table(table) => json!({
            "type": "table",
            "header": table.header,
            "rows": table
                .rows
                .iter()
                .map(|row| row.iter().map(encode).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        }),
        Value::Formula(formula) => {
            let mut obj = json!({
                "type": "formula",
                "latex": formula.latex,
                "display": formula.display,
            });
            if let Some(unit) = &formula.unit {
                obj["unit"] = Json::String(unit.clone());
            }
            if let Some(span) = formula.span {
                obj["span"] = encode_span(span);
            }
            obj
        }
        Value::Code(code) => {
            let mut obj = json!({
                "type": "code",
                "language": code.language,
                "source": code.source,
                "content_hash": code.content_hash,
            });
            if let Some(unit) = &code.unit {
                obj["unit"] = Json::String(unit.clone());
            }
            if let Some(span) = code.span {
                obj["span"] = encode_span(span);
            }
            obj
        }
        Value::FunctionRef(func) => json!({
            "type": "function_ref",
            "module": func.module,
            "name": func.name,
        }),
        Value::PluginHandle(handle) => json!({
            "type": "plugin_handle",
            "plugin": handle.plugin,
            "export": handle.export,
            "id": handle.id,
        }),
    }
}

fn encode_span(span: Span) -> Json {
    json!({"start": span.start, "end": span.end})
}

/// Decode a value from wire data using default limits.
pub fn from_wire(json: &Json) -> Result<Value, WireError> {
    from_wire_with_limits(json, WireLimits::default())
}

/// Decode a value from wire data with explicit limits.
pub fn from_wire_with_limits(json: &Json, limits: WireLimits) -> Result<Value, WireError> {
    decode(json, 0, &limits)
}

fn decode(json: &Json, depth: usize, limits: &WireLimits) -> Result<Value, WireError> {
    if depth > limits.max_depth {
        return Err(WireError::LimitExceeded("nesting depth"));
    }
    let obj = json
        .as_object()
        .ok_or_else(|| WireError::Invalid("value must be a JSON object".into()))?;
    let type_tag = obj
        .get("type")
        .and_then(Json::as_str)
        .ok_or(WireError::MissingField("type"))?;
    let require = |field: &'static str| obj.get(field).ok_or(WireError::MissingField(field));
    match type_tag {
        "unit" => Ok(Value::Unit),
        "none" => Ok(Value::None),
        "bool" => require("value")?
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| WireError::Invalid("bool.value must be a boolean".into())),
        "int" => {
            let decimal = require("decimal")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("int.decimal must be a string".into()))?;
            decimal
                .parse::<BigInt>()
                .map(Value::Int)
                .map_err(|_| WireError::Invalid(format!("invalid integer `{decimal}`")))
        }
        "float" => {
            if let Some(special) = obj.get("special").and_then(Json::as_str) {
                let f = match special {
                    "nan" => f64::NAN,
                    "infinity" => f64::INFINITY,
                    "-infinity" => f64::NEG_INFINITY,
                    other => {
                        return Err(WireError::Invalid(format!(
                            "unknown float special `{other}`"
                        )))
                    }
                };
                return Ok(Value::Float(f));
            }
            require("value")?
                .as_f64()
                .map(Value::Float)
                .ok_or_else(|| WireError::Invalid("float.value must be a number".into()))
        }
        "text" => {
            let text = require("value")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("text.value must be a string".into()))?;
            if text.len() > limits.max_string_bytes {
                return Err(WireError::LimitExceeded("text length"));
            }
            Ok(Value::Text(text.to_string()))
        }
        "list" => {
            let items = require("items")?
                .as_array()
                .ok_or_else(|| WireError::Invalid("list.items must be an array".into()))?;
            if items.len() > limits.max_items {
                return Err(WireError::LimitExceeded("list length"));
            }
            let decoded = items
                .iter()
                .map(|item| decode(item, depth + 1, limits))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(decoded))
        }
        "map" => {
            let entries = require("entries")?
                .as_object()
                .ok_or_else(|| WireError::Invalid("map.entries must be an object".into()))?;
            if entries.len() > limits.max_items {
                return Err(WireError::LimitExceeded("map length"));
            }
            let mut map = BTreeMap::new();
            for (key, val) in entries {
                if key.len() > limits.max_string_bytes {
                    return Err(WireError::LimitExceeded("map key length"));
                }
                map.insert(key.clone(), decode(val, depth + 1, limits)?);
            }
            Ok(Value::Map(map))
        }
        "table" => {
            let header = require("header")?
                .as_array()
                .ok_or_else(|| WireError::Invalid("table.header must be an array".into()))?
                .iter()
                .map(|cell| {
                    cell.as_str().map(str::to_string).ok_or_else(|| {
                        WireError::Invalid("table.header cells must be strings".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let rows = require("rows")?
                .as_array()
                .ok_or_else(|| WireError::Invalid("table.rows must be an array".into()))?
                .iter()
                .map(|row| {
                    let cells = row
                        .as_array()
                        .ok_or_else(|| WireError::Invalid("table rows must be arrays".into()))?;
                    if cells.len() != header.len() {
                        return Err(WireError::Invalid(
                            "table row length does not match header".into(),
                        ));
                    }
                    cells
                        .iter()
                        .map(|cell| decode(cell, depth + 1, limits))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Table(Table { header, rows }))
        }
        "formula" => Ok(Value::Formula(FormulaValue {
            latex: require("latex")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("formula.latex must be a string".into()))?
                .to_string(),
            display: obj.get("display").and_then(Json::as_bool).unwrap_or(false),
            unit: optional_string(obj, "unit")?,
            span: optional_span(obj)?,
        })),
        "code" => Ok(Value::Code(CodeValue {
            language: optional_string(obj, "language")?.unwrap_or_default(),
            source: require("source")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("code.source must be a string".into()))?
                .to_string(),
            unit: optional_string(obj, "unit")?,
            content_hash: optional_string(obj, "content_hash")?.unwrap_or_default(),
            span: optional_span(obj)?,
        })),
        "function_ref" => Ok(Value::FunctionRef(FunctionRef {
            module: require("module")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("function_ref.module must be a string".into()))?
                .to_string(),
            name: require("name")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("function_ref.name must be a string".into()))?
                .to_string(),
        })),
        "plugin_handle" => Ok(Value::PluginHandle(PluginHandle {
            plugin: require("plugin")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("plugin_handle.plugin must be a string".into()))?
                .to_string(),
            export: require("export")?
                .as_str()
                .ok_or_else(|| WireError::Invalid("plugin_handle.export must be a string".into()))?
                .to_string(),
            id: require("id")?
                .as_u64()
                .ok_or_else(|| WireError::Invalid("plugin_handle.id must be an integer".into()))?,
        })),
        other => Err(WireError::UnknownType(other.to_string())),
    }
}

fn optional_string(obj: &Map<String, Json>, field: &str) -> Result<Option<String>, WireError> {
    match obj.get(field) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(WireError::Invalid(format!("{field} must be a string"))),
    }
}

fn optional_span(obj: &Map<String, Json>) -> Result<Option<Span>, WireError> {
    let Some(span) = obj.get("span") else {
        return Ok(None);
    };
    let start = span
        .get("start")
        .and_then(Json::as_u64)
        .ok_or_else(|| WireError::Invalid("span.start must be an integer".into()))?;
    let end = span
        .get("end")
        .and_then(Json::as_u64)
        .ok_or_else(|| WireError::Invalid("span.end must be an integer".into()))?;
    Ok(Some(Span::new(start as usize, end as usize)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(value: Value) {
        let wire = to_wire(&value);
        let decoded = from_wire(&wire).expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn scalar_roundtrip() {
        roundtrip(Value::Unit);
        roundtrip(Value::None);
        roundtrip(Value::Bool(true));
        roundtrip(Value::int(-7));
        roundtrip(Value::Float(1.5));
        roundtrip(Value::Float(-0.0));
        roundtrip(Value::Text("你好\n".into()));
        roundtrip(Value::List(vec![Value::int(1), Value::text("x")]));
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), Value::Bool(false));
        roundtrip(Value::Map(map));
        roundtrip(Value::Table(Table {
            header: vec!["x".into()],
            rows: vec![vec![Value::int(1)]],
        }));
    }

    #[test]
    fn big_integers_keep_all_digits() {
        let big: BigInt = "123456789012345678901234567890123456789".parse().unwrap();
        let wire = to_wire(&Value::Int(big));
        assert_eq!(wire["decimal"], "123456789012345678901234567890123456789");
        assert!(matches!(from_wire(&wire), Ok(Value::Int(_))));
    }

    #[test]
    fn special_floats_are_tagged() {
        assert_eq!(to_wire(&Value::Float(f64::NAN))["special"], "nan");
        assert_eq!(to_wire(&Value::Float(f64::INFINITY))["special"], "infinity");
        let decoded = from_wire(&to_wire(&Value::Float(f64::NEG_INFINITY))).unwrap();
        assert!(matches!(decoded, Value::Float(f) if f == f64::NEG_INFINITY));
    }

    #[test]
    fn untrusted_wire_is_validated() {
        assert_eq!(
            from_wire(&json!({"type": "int", "decimal": "not-a-number"})),
            Err(WireError::Invalid("invalid integer `not-a-number`".into()))
        );
        assert_eq!(
            from_wire(&json!({"type": "mystery"})),
            Err(WireError::UnknownType("mystery".into()))
        );
        let deep = (0..100).fold(
            json!({"type": "unit"}),
            |acc, _| json!({"type": "list", "items": [acc]}),
        );
        assert_eq!(
            from_wire(&deep),
            Err(WireError::LimitExceeded("nesting depth"))
        );
    }
}
