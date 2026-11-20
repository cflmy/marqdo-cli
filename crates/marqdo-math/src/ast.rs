//! Formula AST, exact-number representation and deterministic rendering.
//!
//! The AST is public so that `math.inspect` and tests can observe the parsed
//! structure instead of re-parsing the source string. [`Formula::source`] keeps
//! the original LaTeX exactly as supplied to [`crate::parse`]; rendering is only
//! used when an operation (currently [`crate::substitute`]) produces a new
//! formula.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::Ratio;
use num_traits::{One, Signed, Zero};

/// Exact rational number used internally by the kernel.
///
/// This is `num-rational`'s generic `Ratio` instantiated over the same
/// `num-bigint` `BigInt` type used by [`marqdo_value::Value::Int`], so values can
/// cross the kernel boundary without a lossy or string round-trip.
pub type Rational = Ratio<BigInt>;

/// Named mathematical constants supported by the subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constant {
    /// The circle ratio, written `\pi` or `pi`.
    Pi,
    /// Euler's number, written `\e` or `e`.
    E,
}

impl Constant {
    /// The approximate `f64` value of this constant.
    pub fn value(self) -> f64 {
        match self {
            Constant::Pi => std::f64::consts::PI,
            Constant::E => std::f64::consts::E,
        }
    }
}

/// A node of the supported expression grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// An exact numeric literal. Decimal literals are reduced to exact
    /// rationals (for example `0.25` becomes `1/4`).
    Number(Rational),
    /// A free variable resolved from the binding map at evaluation time.
    Variable(String),
    /// A named constant (`\pi`, `pi`, `\e`, `e`).
    Constant(Constant),
    /// Unary negation.
    Neg(Box<Expr>),
    /// Addition.
    Add(Box<Expr>, Box<Expr>),
    /// Subtraction.
    Sub(Box<Expr>, Box<Expr>),
    /// Multiplication.
    Mul(Box<Expr>, Box<Expr>),
    /// Division. `\frac{a}{b}` is parsed as `Div(a, b)`.
    Div(Box<Expr>, Box<Expr>),
    /// Exponentiation. The exponent must evaluate to an integer.
    Pow(Box<Expr>, Box<Expr>),
    /// Square root. `\sqrt{x}` is a dedicated node so that perfect squares can
    /// stay exact.
    Sqrt(Box<Expr>),
}

impl Expr {
    /// Collect every free variable name (constants are not variables).
    pub fn variables(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        self.collect_variables(&mut names);
        names
    }

    fn collect_variables(&self, names: &mut BTreeSet<String>) {
        match self {
            Expr::Number(_) | Expr::Constant(_) => {}
            Expr::Variable(name) => {
                names.insert(name.clone());
            }
            Expr::Neg(inner) | Expr::Sqrt(inner) => inner.collect_variables(names),
            Expr::Add(left, right)
            | Expr::Sub(left, right)
            | Expr::Mul(left, right)
            | Expr::Div(left, right)
            | Expr::Pow(left, right) => {
                left.collect_variables(names);
                right.collect_variables(names);
            }
        }
    }

    /// Number of syntactic occurrences of `name`.
    pub fn occurrences(&self, name: &str) -> usize {
        match self {
            Expr::Number(_) | Expr::Constant(_) => 0,
            Expr::Variable(variable) => usize::from(variable == name),
            Expr::Neg(inner) | Expr::Sqrt(inner) => inner.occurrences(name),
            Expr::Add(left, right)
            | Expr::Sub(left, right)
            | Expr::Mul(left, right)
            | Expr::Div(left, right)
            | Expr::Pow(left, right) => left.occurrences(name) + right.occurrences(name),
        }
    }
}

/// Top level shape of a parsed formula: an expression or a single equation.
///
/// A formula with one `=` is an [`FormulaKind::Equation`]; it is never an
/// assignment. More than one `=` is a parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum FormulaKind {
    /// A standalone arithmetic expression.
    Expression(Expr),
    /// `left = right`.
    Equation {
        /// Left-hand side.
        left: Expr,
        /// Right-hand side.
        right: Expr,
    },
}

/// A parsed formula plus the original source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    /// Original LaTeX source exactly as supplied to [`crate::parse`], or the
    /// canonical rendering produced by [`crate::substitute`].
    pub source: String,
    /// Parsed structure.
    pub kind: FormulaKind,
}

impl Formula {
    /// Whether this formula is an equation.
    pub fn is_equation(&self) -> bool {
        matches!(self.kind, FormulaKind::Equation { .. })
    }

    /// Borrow the expression when this formula is not an equation.
    pub fn expression(&self) -> Option<&Expr> {
        match &self.kind {
            FormulaKind::Expression(expr) => Some(expr),
            FormulaKind::Equation { .. } => None,
        }
    }

    /// Borrow both sides when this formula is an equation.
    pub fn equation(&self) -> Option<(&Expr, &Expr)> {
        match &self.kind {
            FormulaKind::Expression(_) => None,
            FormulaKind::Equation { left, right } => Some((left, right)),
        }
    }

    /// Every free variable appearing in the formula, in sorted order.
    pub fn variables(&self) -> BTreeSet<String> {
        match &self.kind {
            FormulaKind::Expression(expr) => expr.variables(),
            FormulaKind::Equation { left, right } => {
                let mut names = left.variables();
                names.extend(right.variables());
                names
            }
        }
    }

    /// Render this formula back to the supported LaTeX subset deterministically.
    ///
    /// The result is always re-parseable by [`crate::parse`]. This is used by
    /// [`crate::substitute`] to build the new [`Formula::source`].
    pub fn render(&self) -> String {
        match &self.kind {
            FormulaKind::Expression(expr) => render_expr(expr),
            FormulaKind::Equation { left, right } => {
                format!("{} = {}", render_expr(left), render_expr(right))
            }
        }
    }
}

// Rendering precedence levels, from loosest to tightest.
const PREC_ADD: u8 = 1;
const PREC_MUL: u8 = 2;
const PREC_UNARY: u8 = 3;
const PREC_POW: u8 = 4;
const PREC_ATOM: u8 = 5;

/// Fractional digits beyond which a terminating rational is rendered as
/// `\frac{..}{..}` instead of a long decimal. Chosen so that substituted
/// `f64` values that are exactly representable in a short decimal stay readable,
/// while binary-exact values with long expansions use a compact fraction.
const MAX_RENDERED_DECIMAL_DIGITS: u32 = 32;

/// Render an expression without surrounding parentheses.
pub fn render_expr(expr: &Expr) -> String {
    render(expr, 0)
}

fn render(expr: &Expr, parent_precedence: u8) -> String {
    match expr {
        Expr::Number(value) => {
            let (text, precedence) = render_rational(value);
            parenthesize(text, precedence, parent_precedence)
        }
        Expr::Variable(name) => name.clone(),
        Expr::Constant(constant) => match constant {
            Constant::Pi => "\\pi".to_string(),
            Constant::E => "\\e".to_string(),
        },
        Expr::Neg(inner) => {
            let text = format!("-{}", render(inner, PREC_UNARY));
            parenthesize(text, PREC_UNARY, parent_precedence)
        }
        Expr::Add(left, right) => {
            let text = format!(
                "{} + {}",
                render(left, PREC_ADD),
                render(right, PREC_ADD + 1)
            );
            parenthesize(text, PREC_ADD, parent_precedence)
        }
        Expr::Sub(left, right) => {
            let text = format!(
                "{} - {}",
                render(left, PREC_ADD),
                render(right, PREC_ADD + 1)
            );
            parenthesize(text, PREC_ADD, parent_precedence)
        }
        Expr::Mul(left, right) => {
            let text = format!(
                "{} * {}",
                render(left, PREC_MUL),
                render(right, PREC_MUL + 1)
            );
            parenthesize(text, PREC_MUL, parent_precedence)
        }
        Expr::Div(left, right) => {
            let text = format!(
                "{} / {}",
                render(left, PREC_MUL),
                render(right, PREC_MUL + 1)
            );
            parenthesize(text, PREC_MUL, parent_precedence)
        }
        Expr::Pow(base, exponent) => {
            let text = format!(
                "{} ^ {}",
                render(base, PREC_POW + 1),
                render(exponent, PREC_POW)
            );
            parenthesize(text, PREC_POW, parent_precedence)
        }
        Expr::Sqrt(inner) => format!("\\sqrt{{{}}}", render(inner, 0)),
    }
}

fn parenthesize(text: String, precedence: u8, parent_precedence: u8) -> String {
    if precedence < parent_precedence {
        format!("({text})")
    } else {
        text
    }
}

/// Render an exact rational, returning the text and its rendering precedence.
///
/// Integral values render as integers. Values with a terminating decimal
/// expansion of at most [`MAX_RENDERED_DECIMAL_DIGITS`] digits render as
/// decimals; everything else renders as `\frac{numerator}{denominator}`.
fn render_rational(value: &Rational) -> (String, u8) {
    if value.is_integer() {
        let integer = value.to_integer();
        let negative = integer.sign() == num_bigint::Sign::Minus;
        let precedence = if negative { PREC_UNARY } else { PREC_ATOM };
        return (integer.to_string(), precedence);
    }

    if let Some(places) = decimal_places(value.denom()) {
        if places <= MAX_RENDERED_DECIMAL_DIGITS {
            let scale = BigInt::from(10_u32).pow(places);
            let scaled = value.numer() * &scale / value.denom();
            let negative = scaled.sign() == num_bigint::Sign::Minus;
            let digits = scaled.abs().to_string();
            let places = places as usize;
            let decimal = if digits.len() <= places {
                format!("0.{}{digits}", "0".repeat(places - digits.len()))
            } else {
                let split = digits.len() - places;
                format!("{}.{}", &digits[..split], &digits[split..])
            };
            let precedence = if negative { PREC_UNARY } else { PREC_ATOM };
            let text = if negative {
                format!("-{decimal}")
            } else {
                decimal
            };
            return (text, precedence);
        }
    }

    (
        format!("\\frac{{{}}}{{{}}}", value.numer(), value.denom()),
        PREC_ATOM,
    )
}

/// Number of fractional digits needed for the terminating decimal expansion of
/// `denominator`, or `None` when the expansion does not terminate or is longer
/// than [`MAX_RENDERED_DECIMAL_DIGITS`].
fn decimal_places(denominator: &BigInt) -> Option<u32> {
    let mut reduced = denominator.clone();
    let two = BigInt::from(2_u32);
    let five = BigInt::from(5_u32);
    let mut twos = 0_u32;
    while reduced.is_positive() && (&reduced % &two).is_zero() {
        reduced /= &two;
        twos += 1;
        if twos > MAX_RENDERED_DECIMAL_DIGITS {
            return None;
        }
    }
    let mut fives = 0_u32;
    while reduced.is_positive() && (&reduced % &five).is_zero() {
        reduced /= &five;
        fives += 1;
        if fives > MAX_RENDERED_DECIMAL_DIGITS {
            return None;
        }
    }
    if reduced.is_one() {
        Some(twos.max(fives))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(numerator: i64, denominator: i64) -> Expr {
        Expr::Number(Ratio::new(
            BigInt::from(numerator),
            BigInt::from(denominator),
        ))
    }

    #[test]
    fn rendering_round_trips_through_parse() {
        // The renderer must always emit text the parser accepts.
        for source in [
            "1 + 2 * 3",
            "(1 + 2) * 3",
            "2 ^ (3 + 1)",
            "\\frac{1}{3}",
            "-4 ^ 2",
            "2 ^ (-3)",
            "\\sqrt{2}",
            "a = 2 * b + 1",
            "(2 ^ 3) ^ 2",
            "1 - (2 - 3)",
            "a / (b * c)",
        ] {
            let formula = crate::parse(source).expect("fixture parses");
            let rendered = formula.render();
            let reparsed = crate::parse(&rendered)
                .unwrap_or_else(|err| panic!("rendered {rendered:?} does not parse: {err}"));
            assert_eq!(reparsed.kind, formula.kind, "source {source:?}");
        }
    }

    #[test]
    fn decimal_and_fraction_rendering_is_deterministic() {
        assert_eq!(render_expr(&number(5, 2)), "2.5");
        assert_eq!(render_expr(&number(-5, 2)), "-2.5");
        assert_eq!(render_expr(&number(1, 8)), "0.125");
        assert_eq!(render_expr(&number(1, 3)), "\\frac{1}{3}");
        assert_eq!(render_expr(&number(0, 1)), "0");
    }

    #[test]
    fn variables_are_collected_without_constants() {
        let formula = crate::parse("2 * x + y * \\pi").unwrap();
        assert_eq!(
            formula.variables().into_iter().collect::<Vec<_>>(),
            vec!["x".to_string(), "y".to_string()]
        );
        assert_eq!(formula.expression().unwrap().occurrences("x"), 1);
    }
}
