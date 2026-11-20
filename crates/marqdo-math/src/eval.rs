//! Exact evaluation, equation solving and substitution.
//!
//! Numeric semantics are documented on the crate root. In short: literals,
//! integer/float bindings and the operators `+ - * / ^` are evaluated as exact
//! rationals; `\pi`, `\e` and non-perfect square roots introduce an `f64`
//! approximation. The final value is [`Value::Int`] exactly when the exact
//! result is an integer and [`Value::Float`] otherwise.

use std::collections::BTreeMap;

use num_bigint::{BigInt, Sign};
use num_rational::Ratio;
use num_traits::{One, Signed, ToPrimitive, Zero};

use marqdo_value::Value;

use crate::ast::{Expr, Formula, FormulaKind, Rational};
use crate::error::MathError;

/// Maximum magnitude of an integer exponent accepted by `^`.
pub const MAX_EXPONENT: u64 = 10_000;

/// Maximum number of bits (binary digits) allowed for the numerator and for the
/// denominator of an exact intermediate or result. This bounds memory and CPU
/// for hostile formulas such as `(2^10000)^10000`.
pub const MAX_RESULT_BITS: u64 = 1_000_000;

/// A number during evaluation: either exact or an explicit `f64`
/// approximation produced by an irrational operation.
#[derive(Debug, Clone)]
enum Numeric {
    Exact(Rational),
    Approx(f64),
}

impl Numeric {
    fn exact_zero() -> Self {
        Numeric::Exact(Ratio::from_integer(BigInt::zero()))
    }

    fn exact_one() -> Self {
        Numeric::Exact(Ratio::from_integer(BigInt::one()))
    }

    fn is_zero(&self) -> bool {
        match self {
            Numeric::Exact(value) => value.is_zero(),
            Numeric::Approx(value) => *value == 0.0,
        }
    }

    fn neg(self) -> Result<Self, MathError> {
        match self {
            Numeric::Exact(value) => Ok(Numeric::Exact(-value)),
            Numeric::Approx(value) => Ok(Numeric::Approx(-value)),
        }
    }

    fn to_approx(&self) -> Result<f64, MathError> {
        match self {
            Numeric::Approx(value) => Ok(*value),
            Numeric::Exact(value) => ratio_to_f64(value).ok_or_else(|| {
                MathError::size_bound(
                    "an exact intermediate result is too large to represent as a Float",
                )
            }),
        }
    }

    fn add(self, other: Numeric) -> Result<Self, MathError> {
        match (self, other) {
            (Numeric::Exact(left), Numeric::Exact(right)) => {
                Ok(Numeric::Exact(checked_ratio(left + right)?))
            }
            (left, right) => Ok(Numeric::Approx(left.to_approx()? + right.to_approx()?)),
        }
    }

    fn sub(self, other: Numeric) -> Result<Self, MathError> {
        match (self, other) {
            (Numeric::Exact(left), Numeric::Exact(right)) => {
                Ok(Numeric::Exact(checked_ratio(left - right)?))
            }
            (left, right) => Ok(Numeric::Approx(left.to_approx()? - right.to_approx()?)),
        }
    }

    fn mul(self, other: Numeric) -> Result<Self, MathError> {
        match (self, other) {
            (Numeric::Exact(left), Numeric::Exact(right)) => {
                Ok(Numeric::Exact(exact_mul(&left, &right)?))
            }
            (left, right) => Ok(Numeric::Approx(left.to_approx()? * right.to_approx()?)),
        }
    }

    fn div(self, other: Numeric) -> Result<Self, MathError> {
        if other.is_zero() {
            return Err(division_by_zero());
        }
        match (self, other) {
            (Numeric::Exact(left), Numeric::Exact(right)) => {
                Ok(Numeric::Exact(checked_ratio(left / right)?))
            }
            (left, right) => Ok(Numeric::Approx(left.to_approx()? / right.to_approx()?)),
        }
    }

    fn sqrt(self) -> Result<Self, MathError> {
        match self {
            Numeric::Exact(value) => {
                if value.is_negative() {
                    return Err(MathError::type_error(
                        "square root of a negative number is not supported by the real-valued subset",
                        None,
                    ));
                }
                let numerator_root = integer_sqrt(value.numer());
                let denominator_root = integer_sqrt(value.denom());
                if &(&numerator_root * &numerator_root) == value.numer()
                    && &(&denominator_root * &denominator_root) == value.denom()
                {
                    Ok(Numeric::Exact(Ratio::new(numerator_root, denominator_root)))
                } else {
                    let approximate = ratio_to_f64(&value).ok_or_else(|| {
                        MathError::size_bound(
                            "square root operands are outside the representable Float range",
                        )
                    })?;
                    Ok(Numeric::Approx(approximate.sqrt()))
                }
            }
            Numeric::Approx(value) => {
                if value < 0.0 {
                    return Err(MathError::type_error(
                        "square root of a negative number is not supported by the real-valued subset",
                        None,
                    ));
                }
                Ok(Numeric::Approx(value.sqrt()))
            }
        }
    }

    fn pow(self, exponent: BigInt) -> Result<Self, MathError> {
        let negative_exponent = exponent.sign() == Sign::Minus;
        let magnitude = exponent.magnitude();
        let bound = num_bigint::BigUint::from(MAX_EXPONENT);
        if *magnitude > bound {
            return Err(MathError::size_bound(format!(
                "exponent magnitude exceeds the supported bound of {MAX_EXPONENT}"
            )));
        }
        let exponent_u32 = magnitude
            .to_u32()
            .ok_or_else(|| MathError::size_bound("exponent is outside the supported bound"))?;

        match self {
            Numeric::Exact(base) => {
                if exponent_u32 == 0 {
                    if base.is_zero() {
                        return Err(MathError::type_error(
                            "0 ^ 0 is undefined in the native math subset",
                            None,
                        ));
                    }
                    return Ok(Numeric::Exact(Ratio::from_integer(BigInt::one())));
                }
                if base.is_zero() && negative_exponent {
                    return Err(division_by_zero());
                }
                let max_bits = base.numer().bits().max(base.denom().bits());
                let estimated_bits = max_bits.saturating_mul(u64::from(exponent_u32));
                if estimated_bits > MAX_RESULT_BITS {
                    return Err(MathError::size_bound(format!(
                        "exponentiation result would exceed the supported size bound of {MAX_RESULT_BITS} bits"
                    )));
                }
                let numerator = base.numer().pow(exponent_u32);
                let denominator = base.denom().pow(exponent_u32);
                let result = if negative_exponent {
                    Ratio::new(denominator, numerator)
                } else {
                    Ratio::new(numerator, denominator)
                };
                Ok(Numeric::Exact(checked_ratio(result)?))
            }
            Numeric::Approx(base) => {
                if base == 0.0 && negative_exponent {
                    return Err(division_by_zero());
                }
                let exponent = exponent_u32 as i32;
                Ok(Numeric::Approx(base.powi(exponent)))
            }
        }
    }

    fn into_value(self) -> Result<Value, MathError> {
        match self {
            Numeric::Exact(value) => {
                if value.is_integer() {
                    Ok(Value::Int(value.to_integer()))
                } else {
                    Ok(Value::Float(ratio_to_f64(&value).ok_or_else(|| {
                        MathError::size_bound("exact result is too large to represent as a Float")
                    })?))
                }
            }
            Numeric::Approx(value) => Ok(Value::Float(value)),
        }
    }
}

fn division_by_zero() -> MathError {
    MathError::type_error(
        "division by zero is not defined in the native math subset",
        None,
    )
}

fn checked_ratio(value: Rational) -> Result<Rational, MathError> {
    if value.numer().bits() > MAX_RESULT_BITS || value.denom().bits() > MAX_RESULT_BITS {
        return Err(MathError::size_bound(format!(
            "intermediate result exceeds the supported size bound of {MAX_RESULT_BITS} bits"
        )));
    }
    Ok(value)
}

/// Convert an exact rational to `f64` without going through `Ratio`'s
/// `ToPrimitive` impl (which is bound to a different `num-bigint` version in
/// `num-rational` 0.4). Returns `None` when the value is outside the finite
/// `f64` range or would lose everything to underflow.
fn ratio_to_f64(value: &Rational) -> Option<f64> {
    if value.is_zero() {
        return Some(0.0);
    }
    let negative = value.numer().sign() == Sign::Minus;
    let (numerator_mantissa, numerator_exponent) = scaled_f64_mantissa(&value.numer().abs());
    let (denominator_mantissa, denominator_exponent) = scaled_f64_mantissa(value.denom());
    let exponent = numerator_exponent - denominator_exponent;
    if exponent > 1023 {
        return None;
    }
    if exponent < -1075 {
        return Some(if negative { -0.0 } else { 0.0 });
    }
    let mut result = numerator_mantissa / denominator_mantissa;
    result *= 2_f64.powi(exponent as i32);
    if !result.is_finite() {
        return None;
    }
    Some(if negative { -result } else { result })
}

/// Decompose a non-negative integer into `(mantissa, exponent)` such that the
/// value is approximately `mantissa * 2^exponent` and `mantissa` fits in `f64`.
fn scaled_f64_mantissa(value: &BigInt) -> (f64, i64) {
    debug_assert!(!value.is_negative());
    let bits = value.bits();
    if bits == 0 {
        return (0.0, 0);
    }
    // Keep the most significant 64 bits; the low bits only affect the last
    // ~11 bits of the f64 mantissa, which is below the documented precision.
    let shift = bits.saturating_sub(64);
    let top = (value >> shift).to_u64().unwrap_or(u64::MAX);
    (top as f64, shift as i64)
}

fn exact_mul(left: &Rational, right: &Rational) -> Result<Rational, MathError> {
    // Pre-check before allocating so that multiplying two large operands cannot
    // allocate an out-of-bound intermediate.
    let numerator_bits = left.numer().bits().saturating_add(right.numer().bits());
    let denominator_bits = left.denom().bits().saturating_add(right.denom().bits());
    if numerator_bits > MAX_RESULT_BITS || denominator_bits > MAX_RESULT_BITS {
        return Err(MathError::size_bound(format!(
            "multiplication result would exceed the supported size bound of {MAX_RESULT_BITS} bits"
        )));
    }
    checked_ratio(left * right)
}

/// Integer square root of a non-negative integer (Newton iteration).
fn integer_sqrt(value: &BigInt) -> BigInt {
    debug_assert!(!value.is_negative());
    if value.is_zero() {
        return BigInt::zero();
    }
    let mut estimate = BigInt::one() << value.bits().div_ceil(2) as usize;
    loop {
        let next = (&estimate + value / &estimate) >> 1;
        if next >= estimate {
            return estimate;
        }
        estimate = next;
    }
}

/// Convert a finite `f64` into the exact binary rational it represents.
fn float_to_ratio(value: f64) -> Option<Rational> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Ratio::from_integer(BigInt::zero()));
    }
    let bits = value.to_bits();
    let negative = (bits >> 63) == 1;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074_i32)
    } else {
        (fraction | (1_u64 << 52), exponent_bits - 1075)
    };
    let mut numerator = BigInt::from(mantissa);
    let mut denominator = BigInt::one();
    if exponent >= 0 {
        numerator <<= exponent as usize;
    } else {
        denominator <<= (-exponent) as usize;
    }
    if negative {
        numerator = -numerator;
    }
    Some(Ratio::new(numerator, denominator))
}

/// Convert a [`Value`] binding into a kernel number.
fn binding_numeric(name: &str, bindings: &BTreeMap<String, Value>) -> Result<Numeric, MathError> {
    match bindings.get(name) {
        None => Err(MathError::undefined_variable(
            format!("variable `{name}` is not bound"),
            None,
        )),
        Some(Value::Int(value)) => Ok(Numeric::Exact(Ratio::from_integer(value.clone()))),
        Some(Value::Float(value)) => float_to_ratio(*value).map(Numeric::Exact).ok_or_else(|| {
            MathError::type_error(
                format!(
                    "binding `{name}` is a non-finite Float; the math kernel only accepts finite numeric bindings"
                ),
                None,
            )
        }),
        Some(other) => Err(MathError::type_error(
            format!(
                "binding `{name}` has type {} but the math kernel requires Int or Float",
                other.type_name()
            ),
            None,
        )),
    }
}

fn eval_expr(expr: &Expr, bindings: &BTreeMap<String, Value>) -> Result<Numeric, MathError> {
    match expr {
        Expr::Number(value) => Ok(Numeric::Exact(value.clone())),
        Expr::Variable(name) => binding_numeric(name, bindings),
        Expr::Constant(constant) => Ok(Numeric::Approx(constant.value())),
        Expr::Neg(inner) => eval_expr(inner, bindings)?.neg(),
        Expr::Add(left, right) => eval_expr(left, bindings)?.add(eval_expr(right, bindings)?),
        Expr::Sub(left, right) => eval_expr(left, bindings)?.sub(eval_expr(right, bindings)?),
        Expr::Mul(left, right) => eval_expr(left, bindings)?.mul(eval_expr(right, bindings)?),
        Expr::Div(left, right) => eval_expr(left, bindings)?.div(eval_expr(right, bindings)?),
        Expr::Pow(base, exponent) => {
            let base = eval_expr(base, bindings)?;
            base.pow(integer_exponent(&eval_expr(exponent, bindings)?)?)
        }
        Expr::Sqrt(inner) => eval_expr(inner, bindings)?.sqrt(),
    }
}

/// Require a numeric value to be an integer and return it.
fn integer_exponent(value: &Numeric) -> Result<BigInt, MathError> {
    match value {
        Numeric::Exact(value) if value.is_integer() => Ok(value.to_integer()),
        _ => Err(MathError::type_error(
            "exponent must be an integer in the native math subset",
            None,
        )),
    }
}

/// A linear/affine form `coefficient * target + constant`.
struct Affine {
    coefficient: Numeric,
    constant: Numeric,
}

impl Affine {
    fn constant(value: Numeric) -> Self {
        Self {
            coefficient: Numeric::exact_zero(),
            constant: value,
        }
    }

    fn target() -> Self {
        Self {
            coefficient: Numeric::exact_one(),
            constant: Numeric::exact_zero(),
        }
    }

    fn is_target_free(&self) -> bool {
        self.coefficient.is_zero()
    }

    fn neg(self) -> Result<Self, MathError> {
        Ok(Self {
            coefficient: self.coefficient.neg()?,
            constant: self.constant.neg()?,
        })
    }

    fn add(self, other: Self) -> Result<Self, MathError> {
        Ok(Self {
            coefficient: self.coefficient.add(other.coefficient)?,
            constant: self.constant.add(other.constant)?,
        })
    }

    fn sub(self, other: Self) -> Result<Self, MathError> {
        Ok(Self {
            coefficient: self.coefficient.sub(other.coefficient)?,
            constant: self.constant.sub(other.constant)?,
        })
    }

    fn scale(self, factor: Numeric) -> Result<Self, MathError> {
        let coefficient = self.coefficient.mul(factor.clone())?;
        let constant = self.constant.mul(factor)?;
        Ok(Self {
            coefficient,
            constant,
        })
    }
}

/// Build the affine form of `expr` with respect to `target`.
///
/// Variables other than `target` are resolved from `bindings`. Operations that
/// are not affine in `target` are rejected as unsupported instead of being
/// approximated or reported as a solution.
fn to_affine(
    expr: &Expr,
    target: &str,
    bindings: &BTreeMap<String, Value>,
) -> Result<Affine, MathError> {
    match expr {
        Expr::Number(value) => Ok(Affine::constant(Numeric::Exact(value.clone()))),
        Expr::Constant(constant) => Ok(Affine::constant(Numeric::Approx(constant.value()))),
        Expr::Variable(name) if name == target => Ok(Affine::target()),
        Expr::Variable(name) => Ok(Affine::constant(binding_numeric(name, bindings)?)),
        Expr::Neg(inner) => to_affine(inner, target, bindings)?.neg(),
        Expr::Add(left, right) => {
            let left = to_affine(left, target, bindings)?;
            let right = to_affine(right, target, bindings)?;
            left.add(right)
        }
        Expr::Sub(left, right) => {
            let left = to_affine(left, target, bindings)?;
            let right = to_affine(right, target, bindings)?;
            left.sub(right)
        }
        Expr::Mul(left, right) => {
            let left = to_affine(left, target, bindings)?;
            let right = to_affine(right, target, bindings)?;
            match (left.is_target_free(), right.is_target_free()) {
                (true, true) => Ok(Affine::constant(
                    left.constant.mul(right.constant)?,
                )),
                (true, false) => right.scale(left.constant),
                (false, true) => left.scale(right.constant),
                (false, false) => Err(MathError::unsupported(
                    format!(
                        "cannot solve for `{target}`: the equation is not linear in the target (it appears in both factors of a product)"
                    ),
                    None,
                )),
            }
        }
        Expr::Div(left, right) => {
            let left = to_affine(left, target, bindings)?;
            let right = to_affine(right, target, bindings)?;
            if !right.is_target_free() {
                return Err(MathError::unsupported(
                    format!(
                        "cannot solve for `{target}`: the target appears in a denominator, which is outside the supported linear forms"
                    ),
                    None,
                ));
            }
            if right.constant.is_zero() {
                return Err(division_by_zero());
            }
            let inverse = Numeric::exact_one().div(right.constant)?;
            left.scale(inverse)
        }
        Expr::Pow(base, exponent) => {
            if exponent.occurrences(target) > 0 {
                return Err(MathError::unsupported(
                    format!(
                        "cannot solve for `{target}`: the target appears in an exponent, which is not an affine form"
                    ),
                    None,
                ));
            }
            let exponent_value = eval_expr(exponent, bindings)?;
            let exponent = integer_exponent(&exponent_value)?;
            let base = to_affine(base, target, bindings)?;
            if base.is_target_free() {
                let value = base.constant.pow(exponent)?;
                return Ok(Affine::constant(value));
            }
            if exponent.is_zero() {
                return Ok(Affine::constant(Numeric::exact_one()));
            }
            if exponent.is_one() {
                return Ok(base);
            }
            Err(MathError::unsupported(
                format!(
                    "cannot solve for `{target}`: the target appears in a power with exponent {exponent}, which is not an affine form"
                ),
                None,
            ))
        }
        Expr::Sqrt(inner) => {
            if inner.occurrences(target) > 0 {
                return Err(MathError::unsupported(
                    format!(
                        "cannot solve for `{target}`: the target appears under a square root, which is not an affine form"
                    ),
                    None,
                ));
            }
            Ok(Affine::constant(eval_expr(inner, bindings)?.sqrt()?))
        }
    }
}

/// Evaluate `formula` with `bindings`.
///
/// For an expression the result is the exact value (or a [`Value::Float`] when
/// the exact value is not integral or an irrational operation was involved).
/// For an equation `target` selects the variable to solve for; it must occur
/// exactly once and the equation must be linear in it.
pub fn evaluate(
    formula: &Formula,
    bindings: &BTreeMap<String, Value>,
    target: Option<&str>,
) -> Result<Value, MathError> {
    match (&formula.kind, target) {
        (FormulaKind::Expression(expr), None) => eval_expr(expr, bindings)?.into_value(),
        (FormulaKind::Expression(_), Some(_)) => Err(MathError::unsupported(
            "a target variable is only meaningful for an equation; the formula is a plain expression",
            None,
        )),
        (FormulaKind::Equation { .. }, None) => Err(MathError::unsupported(
            "evaluating an equation requires a target variable to solve for",
            None,
        )),
        (FormulaKind::Equation { left, right }, Some(target)) => {
            solve_equation(left, right, target, bindings)
        }
    }
}

fn solve_equation(
    left: &Expr,
    right: &Expr,
    target: &str,
    bindings: &BTreeMap<String, Value>,
) -> Result<Value, MathError> {
    if target.is_empty() {
        return Err(MathError::unsupported(
            "target variable name must not be empty",
            None,
        ));
    }
    if target == "pi" || target == "e" {
        return Err(MathError::unsupported(
            format!("`{target}` is a reserved constant, not a solvable variable"),
            None,
        ));
    }
    let occurrences = left.occurrences(target) + right.occurrences(target);
    match occurrences {
        0 => {
            return Err(MathError::unsupported(
                format!("cannot solve for `{target}`: the variable does not occur in the equation"),
                None,
            ));
        }
        1 => {}
        count => {
            return Err(MathError::unsupported(
                format!(
                    "cannot solve for `{target}`: the variable occurs {count} times, but only a single occurrence in an affine equation is supported"
                ),
                None,
            ));
        }
    }

    let left = to_affine(left, target, bindings)?;
    let right = to_affine(right, target, bindings)?;
    let coefficient = left.coefficient.sub(right.coefficient)?;
    let constant = left.constant.sub(right.constant)?;
    if coefficient.is_zero() {
        return Err(MathError::unsupported(
            format!(
                "cannot solve for `{target}`: the equation does not uniquely determine the variable"
            ),
            None,
        ));
    }
    constant.neg()?.div(coefficient)?.into_value()
}

/// Replace every bound variable in `formula` with its exact value.
///
/// Unbound variables stay symbolic. The returned formula gets a deterministic,
/// re-parseable [`Formula::source`] rendering; `Float` bindings are inserted as
/// the exact rational value of the `f64` (never as a rounded or lossy decimal).
pub fn substitute(
    formula: &Formula,
    bindings: &BTreeMap<String, Value>,
) -> Result<Formula, MathError> {
    let kind = match &formula.kind {
        FormulaKind::Expression(expr) => FormulaKind::Expression(substitute_expr(expr, bindings)?),
        FormulaKind::Equation { left, right } => FormulaKind::Equation {
            left: substitute_expr(left, bindings)?,
            right: substitute_expr(right, bindings)?,
        },
    };
    let substituted = Formula {
        source: String::new(),
        kind,
    };
    Ok(Formula {
        source: substituted.render(),
        kind: substituted.kind,
    })
}

fn substitute_expr(expr: &Expr, bindings: &BTreeMap<String, Value>) -> Result<Expr, MathError> {
    match expr {
        Expr::Number(_) | Expr::Constant(_) => Ok(expr.clone()),
        Expr::Variable(name) => match bindings.get(name) {
            None => Ok(expr.clone()),
            Some(Value::Int(value)) => Ok(Expr::Number(Ratio::from_integer(value.clone()))),
            Some(Value::Float(value)) => match float_to_ratio(*value) {
                Some(value) => Ok(Expr::Number(value)),
                None => Err(MathError::type_error(
                    format!(
                        "binding `{name}` is a non-finite Float; the math kernel only accepts finite numeric bindings"
                    ),
                    None,
                )),
            },
            Some(other) => Err(MathError::type_error(
                format!(
                    "binding `{name}` has type {} but the math kernel requires Int or Float",
                    other.type_name()
                ),
                None,
            )),
        },
        Expr::Neg(inner) => Ok(Expr::Neg(Box::new(substitute_expr(inner, bindings)?))),
        Expr::Add(left, right) => Ok(Expr::Add(
            Box::new(substitute_expr(left, bindings)?),
            Box::new(substitute_expr(right, bindings)?),
        )),
        Expr::Sub(left, right) => Ok(Expr::Sub(
            Box::new(substitute_expr(left, bindings)?),
            Box::new(substitute_expr(right, bindings)?),
        )),
        Expr::Mul(left, right) => Ok(Expr::Mul(
            Box::new(substitute_expr(left, bindings)?),
            Box::new(substitute_expr(right, bindings)?),
        )),
        Expr::Div(left, right) => Ok(Expr::Div(
            Box::new(substitute_expr(left, bindings)?),
            Box::new(substitute_expr(right, bindings)?),
        )),
        Expr::Pow(base, exponent) => Ok(Expr::Pow(
            Box::new(substitute_expr(base, bindings)?),
            Box::new(substitute_expr(exponent, bindings)?),
        )),
        Expr::Sqrt(inner) => Ok(Expr::Sqrt(Box::new(substitute_expr(inner, bindings)?))),
    }
}
