//! The Marqdo value model.
//!
//! Values are deliberately explicit: there is no truthiness and no implicit
//! string/number conversion. `Unit` (no result) and `None` (explicit empty
//! value) are distinct, as required by the language specification.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

use marqdo_span::Span;

/// A structured GFM table value.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Column headers in order.
    pub header: Vec<String>,
    /// Data rows; every row has exactly `header.len()` cells.
    pub rows: Vec<Vec<Value>>,
}

/// An inert `Formula` value: LaTeX source plus provenance. Parsing and
/// evaluation are separate, explicit operations performed by the math kernel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaValue {
    /// Raw LaTeX (or math extension) source, without delimiters.
    pub latex: String,
    /// Canonical owning unit, when the formula came from a document heading range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Whether the formula was display math (`$$...$$`).
    pub display: bool,
    /// Source location of the formula node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// An inert `Code` value: a fenced code block plus provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeValue {
    /// Fence info string language (empty for plain fences).
    pub language: String,
    /// Raw source inside the fence.
    pub source: String,
    /// Canonical owning unit, when the fence came from a document heading range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Lowercase hex SHA-256 of `source`.
    pub content_hash: String,
    /// Source location of the code block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// A reference to a named function in a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionRef {
    /// Canonical module identity (project relative path or imported alias).
    pub module: String,
    /// Canonical function name within the module.
    pub name: String,
}

/// A host-owned handle to a loaded plugin export. Handles are opaque; they
/// cannot be forged from wire data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHandle {
    /// Plugin identifier from its manifest.
    pub plugin: String,
    /// Export name within the plugin.
    pub export: String,
    /// Host-assigned generation number.
    pub id: u64,
}

/// A Marqdo runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// No result (a function that fell through its body).
    Unit,
    /// Explicit empty value.
    None,
    /// Boolean.
    Bool(bool),
    /// Arbitrary precision integer.
    Int(BigInt),
    /// IEEE-754 double.
    Float(f64),
    /// Unicode text.
    Text(String),
    /// Ordered sequence.
    List(Vec<Value>),
    /// String-keyed record with deterministic key order.
    Map(BTreeMap<String, Value>),
    /// Structured table.
    Table(Table),
    /// Inert formula.
    Formula(FormulaValue),
    /// Inert code fence.
    Code(CodeValue),
    /// Reference to a named function.
    FunctionRef(FunctionRef),
    /// Host-owned plugin handle.
    PluginHandle(PluginHandle),
}

impl Value {
    /// Stable type name used by diagnostics and wire messages.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::None => "None",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Text(_) => "Text",
            Value::List(_) => "List",
            Value::Map(_) => "Map",
            Value::Table(_) => "Table",
            Value::Formula(_) => "Formula",
            Value::Code(_) => "Code",
            Value::FunctionRef(_) => "FunctionRef",
            Value::PluginHandle(_) => "PluginHandle",
        }
    }

    /// Construct an integer from an `i64`.
    pub fn int(value: i64) -> Self {
        Value::Int(BigInt::from(value))
    }

    /// Construct a text value.
    pub fn text(value: impl Into<String>) -> Self {
        Value::Text(value.into())
    }

    /// Whether the value is the boolean `true`.
    pub fn is_true(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// User-facing rendering used by the host `print` operation.
    ///
    /// Strings print unquoted (like a scripting language), collections use a
    /// deterministic Rust-like representation so that output is reproducible.
    pub fn display(&self) -> String {
        match self {
            Value::Unit => "Unit".to_string(),
            Value::None => "None".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Text(t) => t.clone(),
            other => other.repr(),
        }
    }

    /// Debug-style rendering with type-faithful literals.
    pub fn repr(&self) -> String {
        match self {
            Value::Unit => "Unit".to_string(),
            Value::None => "None".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Text(t) => format!("{t:?}"),
            Value::List(items) => {
                let inner = items.iter().map(Value::repr).collect::<Vec<_>>().join(", ");
                format!("[{inner}]")
            }
            Value::Map(map) => {
                let inner = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.repr()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{inner}}}")
            }
            Value::Table(table) => {
                let header = table.header.join(" | ");
                let rows = table
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(Value::display)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("Table[{header}]{{{rows}}}")
            }
            Value::Formula(f) => format!("Formula({:?})", f.latex),
            Value::Code(c) => format!("Code({:?}, {} bytes)", c.language, c.source.len()),
            Value::FunctionRef(r) => format!("FunctionRef({}::{})", r.module, r.name),
            Value::PluginHandle(h) => format!("PluginHandle({}::{})", h.plugin, h.export),
        }
    }

    /// Structural equality that treats `Float` NaN as unequal to itself while
    /// keeping `Int`/`Float` distinct.
    pub fn same_value(&self, other: &Value) -> bool {
        self == other
    }
}

/// Format a float without exposing the internal `f64` Debug formatting
/// (`1.0` vs `1`); NaN and infinities get explicit spellings.
pub fn format_float(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else if f == 0.0 && f.is_sign_negative() {
        "-0.0".to_string()
    } else {
        // Rust's Display for f64 produces the shortest representation that
        // round-trips, and always keeps a decimal point for integral values
        // when using `{:?}`; we use `{}` to keep the common case short but
        // preserve `.0` so that the value still reads as a Float.
        let s = format!("{f}");
        if s.contains('.') || s.contains('e') || s.contains('E') {
            s
        } else {
            format!("{s}.0")
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::Text(value.to_string())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::Text(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::int(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Value::List(value)
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<T: IntoIterator<Item = Value>>(iter: T) -> Self {
        Value::List(iter.into_iter().collect())
    }
}

/// Convert a float to an exact integer when it is integral; used by explicit
/// math coercions, never implicitly by the evaluator.
pub fn float_to_bigint(f: f64) -> Option<BigInt> {
    if f.is_finite() && f.fract() == 0.0 {
        // Route through the decimal string so that values beyond i64 range do
        // not lose precision.
        let s = format!("{f:.0}");
        s.parse().ok()
    } else {
        None
    }
}

/// Convert an integer to `f64` when it round-trips exactly.
pub fn bigint_to_exact_f64(i: &BigInt) -> Option<f64> {
    let f = i.to_f64()?;
    if float_to_bigint(f).as_ref() == Some(i) {
        Some(f)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_none_are_distinct() {
        assert_ne!(Value::Unit, Value::None);
        assert_eq!(Value::Unit.type_name(), "Unit");
        assert_eq!(Value::None.type_name(), "None");
    }

    #[test]
    fn display_is_stable() {
        assert_eq!(Value::int(42).display(), "42");
        assert_eq!(Value::Float(2.0).display(), "2.0");
        assert_eq!(Value::Text("hi".into()).display(), "hi");
        assert_eq!(Value::Text("hi".into()).repr(), "\"hi\"");
        assert_eq!(
            Value::List(vec![Value::int(1), Value::Unit]).display(),
            "[1, Unit]"
        );
    }

    #[test]
    fn big_integers_are_arbitrary_precision() {
        let big: BigInt = "123456789012345678901234567890".parse().unwrap();
        let value = Value::Int(big.clone());
        assert_eq!(value.display(), "123456789012345678901234567890");
        assert_eq!(
            big + BigInt::from(1),
            "123456789012345678901234567891".parse().unwrap()
        );
    }

    #[test]
    fn float_to_bigint_requires_integral_value() {
        assert_eq!(float_to_bigint(2.0), Some(BigInt::from(2)));
        assert_eq!(float_to_bigint(2.5), None);
        assert_eq!(
            bigint_to_exact_f64(&BigInt::from(9007199254740992i64)),
            Some(9007199254740992.0)
        );
        assert_eq!(
            bigint_to_exact_f64(&BigInt::from(9007199254740993i64)),
            None
        );
    }
}
