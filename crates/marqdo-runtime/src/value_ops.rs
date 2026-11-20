//! Numeric and structural operations.
//!
//! Numeric semantics follow spec/evaluation.md §2: Int arithmetic is exact,
//! `/` always yields Float, mixed arithmetic yields Float, and Int/Float
//! comparisons are exact (the float is treated as the binary rational it is).

use std::cmp::Ordering;

use marqdo_expression::{BinaryOp, UnaryOp};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

/// Maximum exponent accepted by `**` for exact integer powers.
pub const MAX_EXACT_EXPONENT: i64 = 100_000;
/// Maximum estimated bits of an exact integer power result.
pub const MAX_POWER_BITS: u64 = 1_000_000;

/// Apply a unary operator.
pub fn unary(op: UnaryOp, value: Value, span: Option<Span>) -> Result<Value, RuntimeError> {
    match (op, value) {
        (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
        (UnaryOp::Not, other) => Err(type_error(
            span,
            format!("`not` expects Bool, got {}", other.type_name()),
        )),
        (UnaryOp::Neg, Value::Int(value)) => Ok(Value::Int(-value)),
        (UnaryOp::Neg, Value::Float(value)) => Ok(Value::Float(-value)),
        (UnaryOp::Neg, other) => Err(type_error(
            span,
            format!("unary `-` expects a number, got {}", other.type_name()),
        )),
        (UnaryOp::Pos, value @ Value::Int(_)) | (UnaryOp::Pos, value @ Value::Float(_)) => {
            Ok(value)
        }
        (UnaryOp::Pos, other) => Err(type_error(
            span,
            format!("unary `+` expects a number, got {}", other.type_name()),
        )),
    }
}

/// Apply a binary operator (short-circuit operators are handled by the caller).
pub fn binary(
    op: BinaryOp,
    left: Value,
    right: Value,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    use BinaryOp::*;
    match op {
        Eq | Ne | Lt | Le | Gt | Ge => compare(op, left, right, span),
        Add => {
            if let (Value::Text(a), Value::Text(b)) = (&left, &right) {
                return Ok(Value::Text(format!("{a}{b}")));
            }
            numeric_binary(op, left, right, span)
        }
        Sub | Mul | Div | Pow => numeric_binary(op, left, right, span),
        FloorDiv | Mod => integer_binary(op, left, right, span),
        And | Or => Err(RuntimeError::new(
            ErrorCategory::Internal,
            span,
            "logical operators must be short-circuited by the evaluator",
        )),
    }
}

fn compare(
    op: BinaryOp,
    left: Value,
    right: Value,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    let ordering = match (&left, &right) {
        (Value::Int(_), Value::Int(_))
        | (Value::Int(_), Value::Float(_))
        | (Value::Float(_), Value::Int(_))
        | (Value::Float(_), Value::Float(_)) => numeric_ordering(&left, &right),
        (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Unit, Value::Unit) | (Value::None, Value::None) => Some(Ordering::Equal),
        (Value::None, Value::Unit) | (Value::Unit, Value::None) => Some(Ordering::Less),
        (Value::List(a), Value::List(b)) => {
            return Ok(Value::Bool(match op {
                BinaryOp::Eq => a == b,
                BinaryOp::Ne => a != b,
                _ => {
                    return Err(type_error(
                        span,
                        "List ordering comparisons are not supported".to_string(),
                    ))
                }
            }))
        }
        (Value::Map(a), Value::Map(b)) => {
            return Ok(Value::Bool(match op {
                BinaryOp::Eq => a == b,
                BinaryOp::Ne => a != b,
                _ => {
                    return Err(type_error(
                        span,
                        "Map ordering comparisons are not supported".to_string(),
                    ))
                }
            }))
        }
        (a, b) => {
            if op == BinaryOp::Eq {
                return Ok(Value::Bool(false));
            }
            if op == BinaryOp::Ne {
                return Ok(Value::Bool(true));
            }
            return Err(type_error(
                span,
                format!("cannot compare {} with {}", a.type_name(), b.type_name()),
            ));
        }
    };
    let Some(ordering) = ordering else {
        // NaN: equal compares false, everything else also false.
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(match op {
        BinaryOp::Eq => ordering == Ordering::Equal,
        BinaryOp::Ne => ordering != Ordering::Equal,
        BinaryOp::Lt => ordering == Ordering::Less,
        BinaryOp::Le => ordering != Ordering::Greater,
        BinaryOp::Gt => ordering == Ordering::Greater,
        BinaryOp::Ge => ordering != Ordering::Less,
        _ => unreachable!("compare called with non-comparison op"),
    }))
}

fn numeric_ordering(left: &Value, right: &Value) -> Option<Ordering> {
    match (to_rational(left), to_rational(right)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        _ => match (to_float(left), to_float(right)) {
            (Some(a), Some(b)) => a.partial_cmp(&b),
            _ => None,
        },
    }
}

fn to_rational(value: &Value) -> Option<BigRational> {
    match value {
        Value::Int(value) => Some(BigRational::from_integer(value.clone())),
        Value::Float(value) => BigRational::from_float(*value),
        _ => None,
    }
}

fn to_float(value: &Value) -> Option<f64> {
    match value {
        Value::Int(value) => value.to_f64(),
        Value::Float(value) => Some(*value),
        _ => None,
    }
}

fn numeric_binary(
    op: BinaryOp,
    left: Value,
    right: Value,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    // Exact integer arithmetic when both sides are Int.
    if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
        return match op {
            BinaryOp::Add => Ok(Value::Int(a + b)),
            BinaryOp::Sub => Ok(Value::Int(a - b)),
            BinaryOp::Mul => Ok(Value::Int(a * b)),
            BinaryOp::Div => {
                if b.is_zero() {
                    Err(division_by_zero(span))
                } else {
                    let quotient = BigRational::new(a.clone(), b.clone());
                    Ok(Value::Float(quotient.to_f64().unwrap_or(f64::NAN)))
                }
            }
            BinaryOp::Pow => integer_power(a, b, span),
            _ => Err(RuntimeError::new(
                ErrorCategory::Internal,
                span,
                "unsupported numeric operator",
            )),
        };
    }
    let (Some(a), Some(b)) = (to_float(&left), to_float(&right)) else {
        return Err(type_error(
            span,
            format!(
                "operator `{}` does not accept {} and {}",
                op_symbol(op),
                left.type_name(),
                right.type_name()
            ),
        ));
    };
    Ok(match op {
        BinaryOp::Add => Value::Float(a + b),
        BinaryOp::Sub => Value::Float(a - b),
        BinaryOp::Mul => Value::Float(a * b),
        BinaryOp::Div => {
            if b == 0.0 {
                return Err(division_by_zero(span));
            }
            Value::Float(a / b)
        }
        BinaryOp::Pow => Value::Float(a.powf(b)),
        _ => {
            return Err(RuntimeError::new(
                ErrorCategory::Internal,
                span,
                "unsupported numeric operator",
            ))
        }
    })
}

fn integer_binary(
    op: BinaryOp,
    left: Value,
    right: Value,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    let (Value::Int(a), Value::Int(b)) = (&left, &right) else {
        return Err(type_error(
            span,
            format!(
                "operator `{}` requires Int operands, got {} and {}",
                op_symbol(op),
                left.type_name(),
                right.type_name()
            ),
        ));
    };
    if b.is_zero() {
        return Err(division_by_zero(span));
    }
    let quotient = a / b;
    let remainder = a % b;
    // Floor division: adjust when the truncated quotient is not the floor.
    let floored = if !remainder.is_zero() && (remainder.is_negative() != b.is_negative()) {
        quotient - BigInt::from(1)
    } else {
        quotient
    };
    Ok(match op {
        BinaryOp::FloorDiv => Value::Int(floored),
        BinaryOp::Mod => Value::Int(a - &floored * b),
        _ => {
            return Err(RuntimeError::new(
                ErrorCategory::Internal,
                span,
                "unsupported integer operator",
            ))
        }
    })
}

fn integer_power(
    base: &BigInt,
    exponent: &BigInt,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    let Some(exponent) = exponent.to_i64() else {
        return Err(RuntimeError::new(
            ErrorCategory::ResourceExhausted,
            span,
            "integer exponent is too large",
        ));
    };
    if exponent < 0 {
        return Err(type_error(
            span,
            "negative integer exponents are not exact; use Float operands or `/`".to_string(),
        ));
    }
    if exponent > MAX_EXACT_EXPONENT {
        return Err(RuntimeError::new(
            ErrorCategory::ResourceExhausted,
            span,
            format!("integer exponent exceeds the limit of {MAX_EXACT_EXPONENT}"),
        ));
    }
    let estimated_bits = base.bits().max(1).saturating_mul(exponent as u64);
    if estimated_bits > MAX_POWER_BITS {
        return Err(RuntimeError::new(
            ErrorCategory::ResourceExhausted,
            span,
            "integer power result would be too large",
        ));
    }
    Ok(Value::Int(base.pow(exponent as u32)))
}

fn op_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::FloorDiv => "//",
        BinaryOp::Mod => "%",
        BinaryOp::Pow => "**",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
    }
}

fn type_error(span: Option<Span>, message: String) -> RuntimeError {
    RuntimeError::type_error(span, message)
}

fn division_by_zero(span: Option<Span>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::DivisionByZero, span, "division by zero")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: i64) -> Value {
        Value::int(value)
    }

    #[test]
    fn integer_arithmetic_is_exact() {
        let big = Value::Int("12345678901234567890".parse().unwrap());
        let result = binary(BinaryOp::Mul, big, int(2), None).unwrap();
        assert_eq!(result.display(), "24691357802469135780");
    }

    #[test]
    fn division_always_yields_float() {
        assert_eq!(
            binary(BinaryOp::Div, int(1), int(2), None).unwrap(),
            Value::Float(0.5)
        );
        assert_eq!(
            binary(BinaryOp::Div, int(4), int(2), None).unwrap(),
            Value::Float(2.0)
        );
    }

    #[test]
    fn floor_division_and_modulo_signs() {
        assert_eq!(
            binary(BinaryOp::FloorDiv, int(-7), int(2), None).unwrap(),
            int(-4)
        );
        assert_eq!(
            binary(BinaryOp::Mod, int(-7), int(2), None).unwrap(),
            int(1)
        );
        assert_eq!(
            binary(BinaryOp::FloorDiv, int(7), int(-2), None).unwrap(),
            int(-4)
        );
        assert_eq!(
            binary(BinaryOp::Mod, int(7), int(-2), None).unwrap(),
            int(-1)
        );
    }

    #[test]
    fn mixed_comparisons_are_exact() {
        let huge = Value::Int("9007199254740993".parse().unwrap());
        let float = Value::Float(9007199254740992.0);
        assert_eq!(
            binary(BinaryOp::Gt, huge, float, None).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn text_concatenation_and_power() {
        assert_eq!(
            binary(BinaryOp::Add, Value::text("a"), Value::text("b"), None).unwrap(),
            Value::text("ab")
        );
        assert_eq!(
            binary(BinaryOp::Pow, int(2), int(10), None).unwrap(),
            int(1024)
        );
        assert!(binary(BinaryOp::Pow, int(2), int(-1), None).is_err());
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert_eq!(
            binary(BinaryOp::Div, int(1), int(0), None)
                .unwrap_err()
                .category,
            ErrorCategory::DivisionByZero
        );
    }
}
