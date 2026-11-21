//! Negative conformance tests: executable shapes that must be hard errors.
//!
//! Three suites share this file:
//!
//! * `tests/negative/*.mq.md` — compile-time errors (`marqdo_frontend`). The
//!   expected `*.expected.json` is `{"codes": [...]}` and must match the exact
//!   sorted list of **error** codes. Warnings emitted by the same document are
//!   ignored here (they are asserted separately) so the contract is precise.
//! * `tests/negative/warnings/*.mq.md` — inert-executable warnings. Expected
//!   shape is `{"severity": "warning", "codes": [...]}`; the document must
//!   compile without errors and the exact sorted warning-code multiset must
//!   match.
//! * `tests/negative/runtime/*.mq.md` — documents that compile but fail at
//!   runtime. Expected shape is
//!   `{"function": "...", "args": {...}, "codes": [...]}` plus the optional
//!   `"stack_limit"` budget override. Every entry is executed and the first
//!   runtime error code must match.
//!
//! No fallback to prose is ever accepted: each fixture must produce at least
//! one diagnostic or runtime error with the expected stable code.

use std::fs;
use std::path::{Path, PathBuf};

use marqdo_host::{Capabilities, StdHost};
use marqdo_ir::ModuleId;
use marqdo_runtime::{run, EvaluatedArg};
use marqdo_sandbox::SandboxMode;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/cli has a parent")
        .parent()
        .expect("crates has a parent")
        .to_path_buf()
}

fn fixtures_in(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    paths.sort();
    paths
}

fn expected_path(fixture: &Path) -> PathBuf {
    let name = fixture
        .file_name()
        .expect("fixture has a file name")
        .to_string_lossy();
    let stem = name
        .strip_suffix(".mq.md")
        .unwrap_or_else(|| panic!("fixture `{}` must end in .mq.md", fixture.display()));
    fixture.with_file_name(format!("{stem}.expected.json"))
}

fn read_expected(fixture: &Path) -> serde_json::Value {
    let path = expected_path(fixture);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("missing expectation {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("invalid JSON in {}: {error}", path.display()))
}

fn expected_codes(expected: &serde_json::Value, fixture: &Path) -> Vec<String> {
    expected
        .get("codes")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("{} has no `codes` array", expected_path(fixture).display()))
        .iter()
        .map(|code| {
            code.as_str()
                .unwrap_or_else(|| {
                    panic!("non-string code in {}", expected_path(fixture).display())
                })
                .to_string()
        })
        .collect()
}

/// Compile-time hard errors, exact sorted error-code multiset.
#[test]
fn compile_time_negative_goldens() {
    let dir = repo_root().join("tests").join("negative");
    let fixtures = fixtures_in(&dir);
    assert!(
        fixtures.len() >= 30,
        "the P0 corpus must keep at least 30 hard-error fixtures, found {}",
        fixtures.len()
    );

    for fixture in &fixtures {
        let source = fs::read_to_string(fixture)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture.display()));
        let analysis = marqdo_frontend::analyze_source("index.mq.md", &source);
        let mut actual: Vec<String> = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        actual.sort();

        let expected = expected_codes(&read_expected(fixture), fixture);
        assert!(
            !actual.is_empty(),
            "negative fixture `{}` produced no error; invalid executable shapes must \
             never fall back to prose",
            fixture.display()
        );
        assert_eq!(
            actual,
            expected,
            "error-code mismatch for `{}`\n  expected: {expected:?}\n  actual:   {actual:?}",
            fixture.display()
        );
        // Code lists are contract data: every expected code must really be an
        // error severity (never a warning or info diagnostic).
        for diagnostic in &analysis.diagnostics {
            if expected.contains(&diagnostic.code) {
                assert_eq!(
                    diagnostic.severity,
                    marqdo_value::Severity::Error,
                    "code `{}` in `{}` is not an error",
                    diagnostic.code,
                    fixture.display()
                );
            }
        }
    }
}

/// Inert-executable warnings inside/outside functions.
#[test]
fn warning_goldens() {
    let dir = repo_root().join("tests").join("negative").join("warnings");
    let fixtures = fixtures_in(&dir);
    assert!(!fixtures.is_empty(), "warning corpus must not be empty");

    for fixture in &fixtures {
        let expected = read_expected(fixture);
        assert_eq!(
            expected.get("severity").and_then(serde_json::Value::as_str),
            Some("warning"),
            "{} must declare `\"severity\": \"warning\"`",
            expected_path(fixture).display()
        );
        let source = fs::read_to_string(fixture)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture.display()));
        let analysis = marqdo_frontend::analyze_source("index.mq.md", &source);
        assert!(
            !analysis.has_errors(),
            "warning fixture `{}` must compile without errors",
            fixture.display()
        );
        let mut actual: Vec<String> = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        actual.sort();
        let expected = expected_codes(&expected, fixture);
        assert_eq!(
            actual,
            expected,
            "warning-code mismatch for `{}`: expected {expected:?}, actual {actual:?}",
            fixture.display()
        );
        for diagnostic in &analysis.diagnostics {
            assert!(
                diagnostic.severity == marqdo_value::Severity::Warning,
                "`{}` emitted non-warning diagnostic {}: {}",
                fixture.display(),
                diagnostic.code,
                diagnostic.message
            );
        }
    }
}

/// Runtime error codes for documents that compile but fail while executing.
#[test]
fn runtime_negative_goldens() {
    let dir = repo_root().join("tests").join("negative").join("runtime");
    let fixtures = fixtures_in(&dir);
    assert!(
        !fixtures.is_empty(),
        "runtime negative corpus must not be empty"
    );

    let project_root = std::env::temp_dir().join(format!(
        "marqdo-conformance-negative-{}",
        std::process::id()
    ));
    fs::create_dir_all(&project_root)
        .unwrap_or_else(|error| panic!("cannot create {}: {error}", project_root.display()));

    for fixture in &fixtures {
        let expected = read_expected(fixture);
        let function = expected
            .get("function")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{} has no `function`", expected_path(fixture).display()))
            .to_string();
        let source = fs::read_to_string(fixture)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture.display()));
        let module_id = ModuleId::from("index.mq.md");
        let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), &source);
        assert!(
            !analysis.has_errors(),
            "runtime fixture `{}` must compile; diagnostics: {:?}",
            fixture.display(),
            analysis
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code.clone(), diagnostic.message.clone()))
                .collect::<Vec<_>>()
        );

        let mut program = marqdo_ir::Program::default();
        program.insert(analysis.module.clone());
        program.main_module = Some(module_id.clone());
        bind_program_calls(&mut program);
        let mut host = StdHost::new(
            SandboxMode::Project,
            Capabilities::pure(),
            project_root.clone(),
        );
        if let Some(stack_limit) = expected
            .get("stack_limit")
            .and_then(serde_json::Value::as_u64)
        {
            host.budget.stack_limit = stack_limit as usize;
        }

        let args_value = expected
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let args_object = args_value
            .as_object()
            .unwrap_or_else(|| panic!("`args` must be an object"));
        let mut runtime_args = Vec::new();
        for (name, wire) in args_object {
            let value = marqdo_value::from_wire(wire).unwrap_or_else(|error| {
                panic!(
                    "invalid wire argument `{name}` in {}: {error}",
                    expected_path(fixture).display()
                )
            });
            runtime_args.push(EvaluatedArg::named(
                name.clone(),
                value,
                marqdo_span::Span::default(),
            ));
        }

        let expected_codes = expected_codes(&expected, fixture);
        match run(
            &program,
            &mut host,
            &module_id,
            &function,
            runtime_args,
        ) {
            Ok(outcome) => panic!(
                "runtime fixture `{}` unexpectedly succeeded with {:?}; expected error {expected_codes:?}",
                fixture.display(),
                outcome.value
            ),
            Err(error) => {
                let codes = vec![error.code().to_string()];
                assert_eq!(
                    codes,
                    expected_codes,
                    "runtime error-code mismatch for `{}`: expected {expected_codes:?}, actual {codes:?}",
                    fixture.display()
                );
            }
        }
    }
    let _ = fs::remove_dir_all(&project_root);
}

/// Bind direct-runtime fixtures exactly like the CLI composition root.
fn bind_program_calls(program: &mut marqdo_ir::Program) {
    let registry = program.callable_registry();
    let bound = marqdo_frontend::calls::bind_program_calls(program, &registry);
    assert!(
        bound.diagnostics.is_empty(),
        "fixture failed static call binding: {:#?}",
        bound.diagnostics
    );
    program.set_call_bindings(bound.bindings);
}
