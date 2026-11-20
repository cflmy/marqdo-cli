//! Conversion of `--arg name=value` CLI arguments into typed values.
//!
//! Arguments use the expression grammar restricted to compile-time constants:
//! `--arg n=3`, `--arg name="小明"`, `--arg flags=[1, 2]`. There is no implicit
//! text fallback: invalid values are usage errors.

use marqdo_expression::{parse_expression, Expr, UnaryOp};
use marqdo_value::Value;

/// Parse one `name=value` argument pair.
pub fn parse_named_arg(raw: &str) -> Result<(String, Value), String> {
    let Some((name, value)) = raw.split_once('=') else {
        return Err(format!("argument `{raw}` must be `name=value`"));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("argument `{raw}` has an empty name"));
    }
    let value = parse_const_value(value.trim())?;
    Ok((name.to_string(), value))
}

/// Parse a constant expression into a value.
pub fn parse_const_value(text: &str) -> Result<Value, String> {
    let expr = parse_expression(text).map_err(|error| {
        format!(
            "invalid argument value `{text}`: {} (strings need double quotes)",
            error.message
        )
    })?;
    const_value(&expr).ok_or_else(|| {
        format!("argument value `{text}` must be a literal constant (no calls or variables)")
    })
}

fn const_value(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Int(value) => Some(Value::Int(value.clone())),
        Expr::Float(value) => Some(Value::Float(*value)),
        Expr::Text(value) => Some(Value::Text(value.clone())),
        Expr::Bool(value) => Some(Value::Bool(*value)),
        Expr::None => Some(Value::None),
        Expr::List(items) => items
            .iter()
            .map(const_value)
            .collect::<Option<Vec<Value>>>()
            .map(Value::List),
        Expr::Record(entries) => {
            let mut map = std::collections::BTreeMap::new();
            for entry in entries {
                map.insert(entry.key.clone(), const_value(&entry.value)?);
            }
            Some(Value::Map(map))
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
            ..
        } => match const_value(operand)? {
            Value::Int(value) => Some(Value::Int(-value)),
            Value::Float(value) => Some(Value::Float(-value)),
            _ => None,
        },
        Expr::Unary {
            op: UnaryOp::Pos,
            operand,
            ..
        } => const_value(operand),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_arguments() {
        assert_eq!(parse_named_arg("n=3").unwrap().1, Value::int(3));
        assert_eq!(
            parse_named_arg("s=\"你好\"").unwrap().1,
            Value::text("你好")
        );
        assert_eq!(parse_named_arg("f=-1.5").unwrap().1, Value::Float(-1.5));
        assert!(matches!(
            parse_named_arg("l=[1, 2]").unwrap().1,
            Value::List(_)
        ));
    }

    #[test]
    fn rejects_non_constant_and_malformed_arguments() {
        assert!(parse_named_arg("n=f(1)").is_err());
        assert!(parse_named_arg("n=裸文本").is_err());
        assert!(parse_named_arg("plain").is_err());
    }
}
