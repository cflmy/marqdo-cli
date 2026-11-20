//! Native Formula kernel for a deliberately limited, documented math subset.
//!
//! This crate is the native math kernel of the Marqdo toolchain. A Markdown math
//! node becomes an inert [`marqdo_value::FormulaValue`]; nothing is evaluated
//! implicitly. The host layer calls [`parse`], [`substitute`] and [`evaluate`]
//! explicitly, and evaluation is gated by the `math.*` capabilities.
//!
//! **The kernel never proves anything.** It computes exact or floating values
//! and solves a restricted affine equation. It does not establish a theorem,
//! does not simplify a formula into a proof obligation and must not be presented
//! as a formal verifier. See the blueprint section 5 ("原生 Math") and
//! `spec/evaluation.md`.
//!
//! # Guarantees
//!
//! * No panics on untrusted LaTeX. Malformed input, unsupported constructs,
//!   unbounded work and wrong value types all become a structured [`MathError`].
//! * Exact rational arithmetic (`num-rational` `Ratio` over `num-bigint`) is used
//!   wherever possible; integers are arbitrary precision.
//! * A formula is never silently simplified, guessed or partially accepted:
//!   anything outside the grammar below is an error.
//!
//! # Supported grammar
//!
//! The parser accepts exactly the following LaTeX subset (whitespace is
//! insignificant; byte offsets are reported on parse errors):
//!
//! ```text
//! formula     = expr [ "=" expr ] ;          single '=' only, never assignment
//! expr        = term { ("+" | "-") term } ;
//! term        = unary { ("*" | "/" | "·" | "×" | implicit) unary } ;
//! unary       = ("+" | "-") unary | power ;
//! power       = primary [ "^" unary ] ;      right associative
//! primary     = number
//!             | identifier                    Unicode letter (plus digits/underscore)
//!             | "\pi" | "pi" | "\e" | "e"     constants
//!             | "(" expr ")" | "\left(" expr "\right)"
//!             | "{" expr "}"                  grouping
//!             | "\frac{" expr "}{" expr "}"    exact division
//!             | "\sqrt{" expr "}"             real square root
//!             ;
//! number      = digits [ "." digits ] | "." digits ;   decimal, no exponent form
//! ```
//!
//! Notes on the grammar:
//!
//! * `\cdot` and `\times` are accepted as synonyms for `*`.
//! * Implicit multiplication is accepted between adjacent factors where the
//!   meaning is unambiguous: `2x`, `\pi r^2`, `(a)(b)` and `m a`. Concatenated
//!   letters form one identifier (`ma` is the variable `ma`; write `m a` or
//!   `m * a` for a product). Two adjacent number literals are a parse error.
//! * Unicode identifiers are supported (for example `面积`, `r`, `速度`). Plain
//!   multi-letter identifiers are allowed as variables, but the names of common
//!   elementary functions (`sin`, `cos`, `log`, `exp`, …) are reserved and
//!   rejected as unsupported so that `sin(x)` is not silently read as a product.
//! * A single top-level `=` creates an equation (`F = m a`). It is **not**
//!   assignment and never writes a binding. A second `=` is a parse error.
//!
//! # Unsupported constructs
//!
//! Everything outside the grammar is a structured [`MathErrorKind::Unsupported`]
//! or [`MathErrorKind::Parse`] error: matrices, sums/integrals, function calls
//! (`\sin`, `\cos`, …), multi-letter commands, `\left[`/`\right]`, mismatched or
//! missing braces, scientific notation (`1e3`), commas, arrays, subscripts,
//! superscript commands other than the `^` operator, `\frac`/`\sqrt` without
//! braced arguments, and so on. Labels, alignment characters and line breaks are
//! likewise unsupported. Underscores are ordinary identifier characters rather
//! than subscripts: `x_1` is the variable named `x_1`, not `x` subscript `1`.
//!
//! # Constants and ambiguity handling
//!
//! * `\pi` and `pi` denote the circle ratio; `\e` and `e` denote Euler's number.
//! * These spellings are **reserved**: a binding with the key `pi` or `e` never
//!   overrides the constant, and they cannot be used as a solve target. Use a
//!   different variable name (for example `E`, `x`, `r`).
//! * `2e` is parsed as `2 * e` (number times the constant); `2e3` is rejected as
//!   scientific notation rather than silently reinterpreted.
//!
//! # Numeric semantics
//!
//! * Decimal literals are exact rationals: `0.25` is `1/4`, not a binary float.
//! * `Int` bindings become exact integers. `Float` bindings are interpreted as
//!   the exact binary rational value of the `f64` (no rounding is introduced);
//!   non-finite `Float` bindings are a [`MathErrorKind::Type`] error.
//! * `+ - * / ^` are exact when both operands are exact. Division keeps a
//!   rational, so `\frac{1}{3} * 3` evaluates exactly to `1`.
//! * The final value is [`marqdo_value::Value::Int`] exactly when the exact result is an
//!   integer, and [`marqdo_value::Value::Float`] otherwise. Consequently `1.5 + 2.5` returns
//!   `Int(4)`, while `1 / 2` returns `Float(0.5)`.
//! * `\pi`, `\e` and a `\sqrt` of a non-perfect square introduce an `f64`
//!   approximation; any result derived from one of them is a [`marqdo_value::Value::Float`].
//!   Precision caveats: approximations follow IEEE-754 binary64, so only about
//!   15–17 significant decimal digits are reliable and repeated operations may
//!   accumulate error. The kernel never claims an approximate result is exact.
//! * `\sqrt` of a perfect square of an exact rational stays exact (`\sqrt{4}` is
//!   `Int(2)`, `\sqrt{1/4}` is `Float(0.5)`); a negative radicand is a type
//!   error because the subset is real-valued.
//! * The exponent of `^` must evaluate to an integer. Its magnitude is bounded by
//!   [`MAX_EXPONENT`]. Integer powers with a negative exponent are exact
//!   reciprocals. `0 ^ 0` is a type error.
//! * Exact intermediate/result numerators and denominators are bounded by
//!   [`MAX_RESULT_BITS`]; exceeding the bound returns a structured
//!   `Unsupported` error instead of hanging or exhausting memory.
//! * Parsing is bounded by [`MAX_SOURCE_BYTES`], [`MAX_PARSE_DEPTH`] and
//!   [`MAX_AST_NODES`]. The node bound also bounds later recursion depth for
//!   long operator chains, so hostile input cannot overflow the stack during
//!   evaluation, rendering or substitution.
//! * Division by zero, `0 ^ 0` and other undefined real operations are
//!   [`MathErrorKind::Type`] errors (the kernel has no dedicated division error
//!   kind, so the host maps the message/kind to `runtime.division_by_zero`).
//!
//! # Equations
//!
//! [`evaluate`] of an equation requires `target`. The target must occur exactly
//! once and the equation must be affine in it. The kernel supports `+`, `-`,
//! multiplication by a target-free side, division where the target is in the
//! numerator, and inversion of those operations in either direction (`2x = 4`,
//! `4 = 2x`, `x / 2 = 3`, `3 = x / 2`). Other variables are substituted from
//! `bindings` first; missing bindings are [`MathErrorKind::UndefinedVariable`].
//!
//! Solving a target that occurs zero times or more than once, that appears in a
//! denominator, under a root, in an exponent or in a non-affine product is a
//! structured [`MathErrorKind::Unsupported`] error. A target passed for a plain
//! expression, or an equation without a target, is likewise `Unsupported`.
//!
//! # Substitution
//!
//! [`substitute`] replaces bound variables with exact numeric literals and keeps
//! unbound variables symbolic. The returned [`Formula::source`] is a
//! deterministic canonical rendering that [`parse`] accepts again. Because the
//! rendering is canonical, it is not byte-identical to the original formatting.

#![forbid(unsafe_code)]

mod ast;
mod error;
mod eval;
mod parse;

pub use ast::{render_expr, Constant, Expr, Formula, FormulaKind, Rational};
pub use error::{MathError, MathErrorKind};
pub use eval::{evaluate, substitute, MAX_EXPONENT, MAX_RESULT_BITS};
pub use parse::{parse, MAX_AST_NODES, MAX_PARSE_DEPTH, MAX_SOURCE_BYTES};

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_value::Value;
    use std::collections::BTreeMap;

    /// End-to-end smoke test for the API shape documented on the crate root.
    #[test]
    fn public_api_round_trip() {
        let formula = parse("A = \\pi r^2").unwrap();
        assert!(formula.is_equation());
        assert_eq!(formula.source, "A = \\pi r^2");

        let mut bindings = BTreeMap::new();
        bindings.insert("r".to_string(), Value::int(2));
        let area = evaluate(&formula, &bindings, Some("A")).unwrap();
        let Value::Float(area) = area else {
            panic!("area should be an approximate Float");
        };
        assert!((area - std::f64::consts::PI * 4.0).abs() < 1e-12);

        let substituted = substitute(&formula, &bindings).unwrap();
        assert_eq!(substituted.source, "A = \\pi * 2 ^ 2");
        assert!(substituted.is_equation());
        assert_eq!(substituted.kind, parse("A = \\pi * 2 ^ 2").unwrap().kind);
    }
}
