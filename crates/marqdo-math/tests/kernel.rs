//! End-to-end tests for the native Formula kernel.
//!
//! These tests exercise the public contract used by the host layer:
//! `parse`, `evaluate` and `substitute`. They cover the documented grammar,
//! exact numeric semantics, equation solving, structured errors and resource
//! bounds.

use std::collections::BTreeMap;

use marqdo_math::{evaluate, parse, substitute, MathError, MathErrorKind, MAX_EXPONENT};
use marqdo_value::Value;
use num_bigint::BigInt;

fn empty() -> BTreeMap<String, Value> {
    BTreeMap::new()
}

fn bindings(entries: &[(&str, Value)]) -> BTreeMap<String, Value> {
    entries
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

fn eval(source: &str, bindings: &BTreeMap<String, Value>) -> Result<Value, MathError> {
    evaluate(&parse(source).expect("fixture must parse"), bindings, None)
}

fn solve(
    source: &str,
    bindings: &BTreeMap<String, Value>,
    target: &str,
) -> Result<Value, MathError> {
    evaluate(
        &parse(source).expect("fixture must parse"),
        bindings,
        Some(target),
    )
}

fn assert_int(value: &Value, expected: i64) {
    assert_eq!(value, &Value::int(expected), "value was {value:?}");
}

fn assert_float_close(value: &Value, expected: f64) {
    let Value::Float(actual) = value else {
        panic!("expected a Float, got {value:?}");
    };
    assert!(
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
        "expected {expected}, got {actual}"
    );
}

#[test]
fn precedence_and_parentheses() {
    assert_int(&eval("1 + 2 * 3", &empty()).unwrap(), 7);
    assert_int(&eval("(1 + 2) * 3", &empty()).unwrap(), 9);
    assert_int(&eval("10 - 3 - 2", &empty()).unwrap(), 5);
    assert_int(&eval("8 / 4 / 2", &empty()).unwrap(), 1);
    assert_int(&eval("2 + 3 * 4 ^ 2", &empty()).unwrap(), 50);
    // Power is right associative, unary minus binds looser than power.
    assert_int(&eval("2 ^ 3 ^ 2", &empty()).unwrap(), 512);
    assert_int(&eval("-2 ^ 2", &empty()).unwrap(), -4);
    assert_int(&eval("(-2) ^ 2", &empty()).unwrap(), 4);
}

#[test]
fn exact_big_integer_arithmetic() {
    let Value::Int(power) = eval("2 ^ 100", &empty()).unwrap() else {
        panic!("expected an Int");
    };
    assert_eq!(power.to_string(), "1267650600228229401496703205376");

    let Value::Int(product) = eval(
        "123456789012345678901234567890 * 987654321098765432109876543210",
        &empty(),
    )
    .unwrap() else {
        panic!("expected an Int");
    };
    assert_eq!(
        product.to_string(),
        "121932631137021795226185032733622923332237463801111263526900"
    );

    let Value::Int(sum) = eval("2 ^ 200 + 1", &empty()).unwrap() else {
        panic!("expected an Int");
    };
    assert_eq!(
        sum.to_string(),
        "1606938044258990275541962092341162602522202993782792835301377"
    );
}

#[test]
fn division_stays_exact_when_it_reduces_to_an_integer() {
    // 1/3 alone is a Float approximation, but three of them are exactly 1.
    assert_int(&eval("(1 / 3) * 3", &empty()).unwrap(), 1);
    assert_int(&eval("(2 / 7) * 7", &empty()).unwrap(), 2);
    assert_int(&eval("100 / 4", &empty()).unwrap(), 25);

    assert_float_close(&eval("10 / 4", &empty()).unwrap(), 2.5);
    assert_float_close(&eval("1 / 3", &empty()).unwrap(), 1.0 / 3.0);
}

#[test]
fn decimal_literals_are_exact_rationals() {
    // 1.5 + 2.5 is exactly 4, which is integral, so the result is an Int.
    assert_int(&eval("1.5 + 2.5", &empty()).unwrap(), 4);
    assert_int(&eval("0.25 * 4", &empty()).unwrap(), 1);
    assert_float_close(&eval("0.1 + 0.2", &empty()).unwrap(), 0.3);
    assert_float_close(&eval(".5 + 1", &empty()).unwrap(), 1.5);
}

#[test]
fn pi_r_squared_area() {
    let mut bindings = empty();
    bindings.insert("r".to_string(), Value::int(2));
    let value = solve("A = \\pi r^2", &bindings, "A").unwrap();
    assert_float_close(&value, std::f64::consts::PI * 4.0);
}

#[test]
fn perfect_and_non_perfect_square_roots() {
    assert_int(&eval("\\sqrt{16}", &empty()).unwrap(), 4);
    assert_int(&eval("\\sqrt{0}", &empty()).unwrap(), 0);
    // \sqrt{1/4} = 1/2: exact but not integral, so it is a Float value.
    assert_float_close(&eval("\\sqrt{\\frac{1}{4}}", &empty()).unwrap(), 0.5);
    // Non-perfect squares are explicitly approximate Floats.
    assert_float_close(
        &eval("\\sqrt{2}", &empty()).unwrap(),
        std::f64::consts::SQRT_2,
    );
    assert_float_close(&eval("\\sqrt{2} * \\sqrt{2}", &empty()).unwrap(), 2.0);
    let err = eval("\\sqrt{-4}", &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Type);
}

#[test]
fn frac_is_exact_division() {
    assert_int(&eval("\\frac{6}{2}", &empty()).unwrap(), 3);
    assert_float_close(&eval("\\frac{6}{4}", &empty()).unwrap(), 1.5);
    assert_float_close(
        &eval("\\frac{\\frac{1}{2}}{\\frac{3}{4}}", &empty()).unwrap(),
        2.0 / 3.0,
    );
    assert_int(&eval("\\frac{1}{3} + \\frac{2}{3}", &empty()).unwrap(), 1);
}

#[test]
fn left_right_parentheses_and_nesting() {
    assert_int(
        &eval(
            "\\left(\\left(1 + 2\\right) * \\left(3 + 4\\right)\\right)",
            &empty(),
        )
        .unwrap(),
        21,
    );
    assert_int(&eval("\\left(1 + 2\\right) * 3", &empty()).unwrap(), 9);
    assert_int(&eval("((((1 + 2)))) * 3", &empty()).unwrap(), 9);
    assert_int(&eval("2 * {3 + 4}", &empty()).unwrap(), 14);
}

#[test]
fn unicode_identifiers() {
    let bindings = bindings(&[("半径", Value::int(3))]);
    assert_float_close(
        &solve("面积 = \\pi 半径^2", &bindings, "面积").unwrap(),
        std::f64::consts::PI * 9.0,
    );
    let substituted = substitute(&parse("半径 + 1").unwrap(), &bindings).unwrap();
    assert_eq!(substituted.source, "3 + 1");
    assert_int(&eval(&substituted.source, &empty()).unwrap(), 4);
}

#[test]
fn constants_and_their_ambiguity_rules() {
    assert_float_close(&eval("\\pi", &empty()).unwrap(), std::f64::consts::PI);
    assert_float_close(&eval("pi", &empty()).unwrap(), std::f64::consts::PI);
    assert_float_close(&eval("\\e", &empty()).unwrap(), std::f64::consts::E);
    assert_float_close(&eval("e", &empty()).unwrap(), std::f64::consts::E);
    // `2e` is 2 times the constant, not scientific notation.
    assert_float_close(&eval("2e", &empty()).unwrap(), 2.0 * std::f64::consts::E);
    // `pi` and `e` are reserved: bindings do not shadow the constants.
    let bindings = bindings(&[("pi", Value::int(3)), ("e", Value::int(3))]);
    assert_float_close(&eval("pi", &bindings).unwrap(), std::f64::consts::PI);
    assert_float_close(&eval("e", &bindings).unwrap(), std::f64::consts::E);
    assert_eq!(
        solve("pi = 3", &empty(), "pi").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
}

#[test]
fn f_equals_m_a_solved_for_m() {
    let bindings = bindings(&[("F", Value::int(12)), ("a", Value::int(3))]);
    assert_int(&solve("F = m a", &bindings, "m").unwrap(), 4);
    // F = m a is an equation, not assignment: solving for m never rebinds F.
    assert_eq!(bindings.get("F"), Some(&Value::int(12)));
}

#[test]
fn f_equals_m_a_solved_for_a() {
    let bindings = bindings(&[("F", Value::int(12)), ("m", Value::int(3))]);
    assert_int(&solve("F = m a", &bindings, "a").unwrap(), 4);
}

#[test]
fn f_equals_m_a_solved_for_f() {
    let bindings = bindings(&[("m", Value::int(3)), ("a", Value::int(4))]);
    assert_int(&solve("F = m a", &bindings, "F").unwrap(), 12);
}

#[test]
fn linear_equation_solving_both_directions() {
    // Target on the left and on the right; multiplication and division by a
    // target-free side are both inverted.
    assert_int(&solve("2 x = 4", &empty(), "x").unwrap(), 2);
    assert_int(&solve("4 = 2 x", &empty(), "x").unwrap(), 2);
    assert_int(&solve("x / 2 = 3", &empty(), "x").unwrap(), 6);
    assert_int(&solve("3 = x / 2", &empty(), "x").unwrap(), 6);
    assert_int(&solve("x + 2 = 5", &empty(), "x").unwrap(), 3);
    assert_int(&solve("2 + x = 5", &empty(), "x").unwrap(), 3);
    assert_int(&solve("2 x + 1 = 7", &empty(), "x").unwrap(), 3);
    assert_int(&solve("x = 4", &empty(), "x").unwrap(), 4);
    assert_int(&solve("x / 4 = 1 / 2", &empty(), "x").unwrap(), 2);
    assert_int(&solve("(x + 1) * 2 = 8", &empty(), "x").unwrap(), 3);

    // Other variables are substituted first.
    let bindings = bindings(&[("y", Value::int(3))]);
    assert_int(&solve("x = 2 y + 1", &bindings, "x").unwrap(), 7);
}

#[test]
fn equation_solving_requires_exactly_one_affine_occurrence() {
    assert_eq!(
        solve("x + x = 4", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("x * x = 4", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("x ^ 2 = 4", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("1 / x = 2", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("\\sqrt{x} = 2", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("2 = 4", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
    assert_eq!(
        solve("x * 0 = 1", &empty(), "x").unwrap_err().kind,
        MathErrorKind::Unsupported
    );
}

#[test]
fn target_errors_are_structured() {
    let err = evaluate(&parse("A = 2").unwrap(), &empty(), None).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);
    assert!(err.message.contains("target"));

    let err = evaluate(&parse("1 + 2").unwrap(), &empty(), Some("x")).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);

    let err = solve("A = 2", &empty(), "B").unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);
    assert!(err.message.contains("does not occur"));

    let err = solve("A = 2", &empty(), "").unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);
}

#[test]
fn unsupported_constructs_are_structured() {
    for source in [
        "\\begin{matrix} 1 & 2 \\\\ 3 & 4 \\end{matrix}",
        "\\int_0^1 x dx",
        "\\sin(x)",
        "\\cos(x) + 1",
        "\\sum_{i=1}^{n} i",
        "\\alpha + 1",
        "\\frac12",
        "1e3",
        "\\left[ 1, 2 \\right]",
    ] {
        let err = parse(source).unwrap_err();
        assert!(
            matches!(err.kind, MathErrorKind::Unsupported | MathErrorKind::Parse),
            "source {source:?} produced {err:?}"
        );
        assert!(
            err.position.is_some(),
            "source {source:?} lost its position"
        );
    }
}

#[test]
fn malformed_latex_is_a_parse_error_with_position() {
    for source in [
        "",
        "1 +",
        "(1 + 2",
        "\\frac{1}{2",
        "\\frac{1}",
        "\\sqrt 2",
        "{1 + 2",
        "1 + 2}",
        "1 + 2)",
        "1.2.3",
        "1 2",
        "a = b = c",
    ] {
        let err = parse(source).unwrap_err();
        assert_eq!(
            err.kind,
            MathErrorKind::Parse,
            "source {source:?} produced {err:?}"
        );
        assert!(err.position.is_some(), "source {source:?}");
    }
}

#[test]
fn undefined_variables_are_reported() {
    let err = eval("1 + x", &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::UndefinedVariable);
    assert!(err.message.contains('x'));

    let err = solve("A = x + 1", &empty(), "A").unwrap_err();
    assert_eq!(err.kind, MathErrorKind::UndefinedVariable);
}

#[test]
fn non_numeric_bindings_are_type_errors() {
    for value in [
        Value::Bool(true),
        Value::text("two"),
        Value::List(vec![Value::int(2)]),
        Value::None,
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
    ] {
        let err = eval("x + 1", &bindings(&[("x", value)])).unwrap_err();
        assert_eq!(err.kind, MathErrorKind::Type);
    }
    // Unused bindings of other types are irrelevant.
    assert_int(
        &eval("1 + 1", &bindings(&[("x", Value::text("unused"))])).unwrap(),
        2,
    );
}

#[test]
fn float_bindings_are_exact_numeric_values() {
    let bindings = bindings(&[("x", Value::Float(2.5))]);
    assert_float_close(&eval("x", &bindings).unwrap(), 2.5);
    // 2.5 + 0.5 is exactly 3, so the exact-result rule yields an Int.
    assert_int(&eval("x + 0.5", &bindings).unwrap(), 3);
    assert_float_close(&eval("x / 2", &bindings).unwrap(), 1.25);
}

#[test]
fn division_by_zero_is_a_structured_error() {
    for source in ["1 / 0", "\\frac{1}{0}", "1 / (2 - 2)"] {
        let err = eval(source, &empty()).unwrap_err();
        assert_eq!(err.kind, MathErrorKind::Type, "source {source:?}");
        assert!(
            err.message.contains("division by zero"),
            "source {source:?} produced {err:?}"
        );
    }
    let err = solve("x / 0 = 1", &empty(), "x").unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Type);
}

#[test]
fn exponent_must_be_an_integer() {
    for source in ["2 ^ 0.5", "2 ^ \\frac{1}{2}", "2 ^ \\pi"] {
        let err = eval(source, &empty()).unwrap_err();
        assert_eq!(err.kind, MathErrorKind::Type, "source {source:?}");
    }
}

#[test]
fn negative_integer_exponents_are_exact_reciprocals() {
    assert_float_close(&eval("2 ^ -3", &empty()).unwrap(), 0.125);
    assert_int(&eval("(1 / 2) ^ -2", &empty()).unwrap(), 4);
    let err = eval("0 ^ -1", &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Type);
}

#[test]
fn huge_exponents_are_bounded_not_hung() {
    let err = eval(&format!("2 ^ {}", MAX_EXPONENT + 1), &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);
    assert!(err.message.contains("exponent"));

    let err = eval("2 ^ 1000000", &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);

    // Each factor is under the exponent bound, but the product size is not.
    let err = eval("(2 ^ 10000) ^ 10000", &empty()).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Unsupported);

    // At the documented exponent bound the result is still computed exactly.
    let Value::Int(bounded) = eval("2 ^ 10000", &empty()).unwrap() else {
        panic!("expected an Int");
    };
    assert_eq!(bounded.bits(), 10001);
}

#[test]
fn substitute_replaces_bindings_deterministically() {
    let formula = parse("A = \\pi r^2").unwrap();
    let bindings = bindings(&[("r", Value::int(2))]);
    let substituted = substitute(&formula, &bindings).unwrap();

    // The rendering is canonical, re-parseable and deterministic.
    assert_eq!(substituted.source, "A = \\pi * 2 ^ 2");
    assert_eq!(substituted.render(), substituted.source);
    assert_eq!(parse(&substituted.source).unwrap().kind, substituted.kind);

    let direct = evaluate(&formula, &bindings, Some("A")).unwrap();
    let via_substitution = evaluate(&substituted, &empty(), Some("A")).unwrap();
    assert_eq!(direct, via_substitution);

    let second = substitute(&formula, &bindings).unwrap();
    assert_eq!(substituted.source, second.source);
}

#[test]
fn substitute_leaves_unbound_variables_symbolic() {
    let formula = parse("x + y * 2").unwrap();
    let substituted = substitute(&formula, &bindings(&[("x", Value::int(3))])).unwrap();
    assert_eq!(substituted.source, "3 + y * 2");
    assert!(substituted.variables().contains("y"));
    assert!(!substituted.variables().contains("x"));

    let value = evaluate(&substituted, &bindings(&[("y", Value::int(4))]), None).unwrap();
    assert_int(&value, 11);
}

#[test]
fn substitute_rejects_unsupported_binding_types() {
    let formula = parse("x + 1").unwrap();
    let err = substitute(&formula, &bindings(&[("x", Value::text("two"))])).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Type);
}

#[test]
fn scientific_notation_and_adjacent_numbers_are_rejected() {
    assert_eq!(parse("1e3").unwrap_err().kind, MathErrorKind::Parse);
    assert_eq!(parse("1E3").unwrap_err().kind, MathErrorKind::Parse);
    assert_eq!(parse("1e-3").unwrap_err().kind, MathErrorKind::Parse);
    assert_eq!(parse("1 2").unwrap_err().kind, MathErrorKind::Parse);
}

#[test]
fn error_display_includes_kind_and_position() {
    let err = parse("\\frac{1}{2").unwrap_err();
    let rendered = err.to_string();
    assert!(rendered.starts_with("parse at byte "), "{rendered}");
    assert!(rendered.contains("expected `}`"), "{rendered}");
}

#[test]
fn hostile_inputs_never_panic() {
    // A small corpus of adversarial inputs: all must produce a structured error
    // (or a value), never a panic or an unbounded computation.
    let sources = [
        "\\",
        "\\\\",
        "\\\\\\\\",
        "\\frac",
        "\\frac{}{}",
        "\\sqrt{}",
        "{}",
        "((((",
        "))))",
        "^^^^",
        "----------------x",
        "1 + + + +",
        "1 // 2",
        "\\left(1",
        "\\right)",
        "\\left[1\\right]",
        "1..2",
        "..",
        "∞",
        "1 × 2",
        "1 · 2",
        "2 ^ 2 ^ 2 ^ 2",
        "\\pi 2 3",
        "x = ",
        "= x",
        "x = = y",
        "\\frac{1}{2} \\frac{3}{4}",
    ];
    for source in sources {
        match parse(source) {
            Ok(formula) => {
                // If it parses, evaluation must be structured as well.
                let _ = evaluate(&formula, &empty(), None);
            }
            Err(err) => {
                assert!(
                    !err.message.is_empty(),
                    "structured error must have a message for {source:?}"
                );
            }
        }
    }
}

#[test]
fn dot_and_times_are_multiplication_operators() {
    assert_int(&eval("2 \\cdot 3", &empty()).unwrap(), 6);
    assert_int(&eval("2 \\times 3", &empty()).unwrap(), 6);
    assert_int(&eval("2 \\cdot 3 \\times 4", &empty()).unwrap(), 24);
}

#[test]
fn solve_target_on_the_left_of_a_product() {
    let bindings = bindings(&[("F", Value::int(20)), ("a", Value::int(5))]);
    assert_int(&solve("m a = F", &bindings, "m").unwrap(), 4);
    assert_int(&solve("(m + 1) a = F", &bindings, "m").unwrap(), 3);
}

#[test]
fn substitution_with_float_and_negative_bindings_reparses() {
    // A Float binding is substituted as its exact numeric value, and the
    // canonical rendering must evaluate to the same value as the direct form.
    let formula = parse("x * 2").unwrap();
    let x_is_float = bindings(&[("x", Value::Float(2.5))]);
    let substituted = substitute(&formula, &x_is_float).unwrap();
    let direct = evaluate(&formula, &x_is_float, None).unwrap();
    let re_evaluated = evaluate(&parse(&substituted.source).unwrap(), &empty(), None).unwrap();
    assert_eq!(direct, re_evaluated);
    assert_int(&direct, 5);

    // Negative substituted literals keep a parseable rendering.
    let formula = parse("x + 1").unwrap();
    let x_is_negative = bindings(&[("x", Value::int(-3))]);
    let substituted = substitute(&formula, &x_is_negative).unwrap();
    let value = evaluate(&parse(&substituted.source).unwrap(), &empty(), None).unwrap();
    assert_int(&value, -2);
}

#[test]
fn large_exact_results_are_returned_as_integers() {
    let Value::Int(value) = eval("2 ^ 10000", &empty()).unwrap() else {
        panic!("expected an Int");
    };
    assert_eq!(value.bits(), 10001);
    // A non-perfect square of the same order remains representable as Float.
    assert!(matches!(
        eval("\\sqrt{2}", &empty()).unwrap(),
        Value::Float(_)
    ));
}

#[test]
fn long_operator_chains_within_the_node_budget_still_evaluate() {
    // Each `+`/`*` needs two leaf nodes plus one operator node, so 200
    // operators stay inside the documented 512-node budget.
    let source = format!("{}1", "1 + ".repeat(200));
    assert_int(&eval(&source, &empty()).unwrap(), 201);

    let source = format!("{}1", "2 * ".repeat(200));
    assert_eq!(
        eval(&source, &empty()).unwrap(),
        Value::Int(BigInt::from(2).pow(200))
    );
}

#[test]
fn oversized_formulas_are_rejected_with_a_structured_error() {
    // Well over the documented node budget, but not deep enough to be caught by
    // the nesting guard: this must be a parse error, not a stack overflow.
    let source = format!("{}1", "1 + ".repeat(200_000));
    let err = parse(&source).unwrap_err();
    assert_eq!(err.kind, MathErrorKind::Parse);
}
