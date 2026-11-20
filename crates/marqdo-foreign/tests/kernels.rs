//! Integration tests against the real interpreters installed in the
//! environment (`python3` and `node`).
//!
//! The tests exercise the public contract the host layer relies on: explicit
//! execution only, the versioned stdin envelope, strict `call` protocol,
//! failure preservation, timeout killing, output limits and detection.

use std::time::Duration;

use marqdo_foreign::{
    call, detect, normalize_language, run, ForeignError, KernelInfo, RunRequest, RunStatus,
};
use marqdo_value::Value;
use serde_json::json;

fn request(language: &str, source: &str) -> RunRequest {
    RunRequest {
        language: language.to_string(),
        source: source.to_string(),
        input: None,
        timeout: Duration::from_secs(30),
        max_output_bytes: 1024 * 1024,
    }
}

fn python(source: &str) -> RunRequest {
    request("python", source)
}

fn node(source: &str) -> RunRequest {
    request("javascript", source)
}

fn kernel(language: &str) -> KernelInfo {
    detect(language).unwrap_or_else(|| panic!("`{language}` kernel must be installed for tests"))
}

#[test]
fn detect_finds_python_and_node_versions() {
    let python = kernel("python");
    assert_eq!(python.language, "python");
    assert!(python.program.is_file());
    assert!(
        python.version.to_ascii_lowercase().contains("python"),
        "unexpected python version string: {:?}",
        python.version
    );

    let alias = kernel("py");
    assert_eq!(alias.language, "python");
    assert!(alias.version.contains("Python"));

    let javascript = kernel("javascript");
    assert_eq!(javascript.language, "javascript");
    assert!(javascript.program.is_file());
    assert!(
        javascript.version.starts_with('v') || !javascript.version.is_empty(),
        "unexpected node version string: {:?}",
        javascript.version
    );

    assert!(detect("node").is_some());
    assert!(detect("cobol").is_none());
    assert_eq!(normalize_language("js"), Some("javascript"));
}

#[test]
fn run_success_captures_stdout() {
    let result = run(&python("print('hello from python')")).expect("run");
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.trim(), "hello from python");
    assert!(result.stderr.is_empty());
    assert!(result.value.is_none());
    assert!(result.duration < Duration::from_secs(20));
}

#[test]
fn stdin_envelope_carries_the_input() {
    let source = r#"import json, sys
req = json.load(sys.stdin)
assert req["wire_version"] == 1, req
assert req["kind"] == "request", req
print(json.dumps({"echo": req["input"]["message"]}))
"#;
    let mut req = python(source);
    req.input = Some(json!({"message": "ping"}));
    let result = run(&req).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    let decoded: serde_json::Value = serde_json::from_str(&result.stdout).expect("json output");
    assert_eq!(decoded["echo"], "ping");
}

#[test]
fn stdin_envelope_sends_null_for_missing_input() {
    let source = r#"import json, sys
req = json.load(sys.stdin)
print(json.dumps({"is_null": req["input"] is None}))
"#;
    let result = run(&python(source)).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    let decoded: serde_json::Value = serde_json::from_str(&result.stdout).expect("json output");
    assert_eq!(decoded["is_null"], true);
}

#[test]
fn call_decodes_a_python_wire_value() {
    let source = r#"import json, sys
json.load(sys.stdin)
print(json.dumps({"type": "int", "decimal": "42"}))
"#;
    let result = call(&python(source)).expect("call");
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.value, Some(Value::int(42)));
}

#[test]
fn call_decodes_a_node_wire_value() {
    let source = r#"import { readFileSync } from "node:fs";
const request = JSON.parse(readFileSync(0, "utf8"));
process.stdout.write(JSON.stringify({ type: "text", value: request.input.message }));
"#;
    let mut req = node(source);
    req.input = Some(json!({"message": "from node"}));
    let result = call(&req).expect("call");
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.value, Some(Value::text("from node")));
}

#[test]
fn node_run_success_captures_stdout() {
    let source = r#"import { readFileSync } from "node:fs";
const request = JSON.parse(readFileSync(0, "utf8"));
console.log(`hello ${request.input.name}`);
"#;
    let mut req = node(source);
    req.input = Some(json!({"name": "node"}));
    let result = run(&req).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "hello node");
}

#[test]
fn node_commonjs_syntax_is_supported() {
    let source = r#"const fs = require("node:fs");
const request = JSON.parse(fs.readFileSync(0, "utf8"));
console.log(`cjs ${request.input.value}`);
"#;
    let mut req = node(source);
    req.input = Some(json!({"value": "ok"}));
    let result = run(&req).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "cjs ok");
}

#[test]
fn non_zero_exit_is_failed_and_keeps_stderr() {
    let source = r#"import sys
print("about to fail", file=sys.stderr)
sys.exit(3)
"#;
    let result = run(&python(source)).expect("run");
    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.exit_code, Some(3));
    assert!(
        result.stderr.contains("about to fail"),
        "stderr was: {:?}",
        result.stderr
    );
    assert!(result.stdout.is_empty());
}

#[test]
fn non_zero_exit_is_preserved_by_call() {
    let source = r#"import sys
print("call failed", file=sys.stderr)
sys.exit(7)
"#;
    let result = call(&python(source)).expect("call");
    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.exit_code, Some(7));
    assert!(result.stderr.contains("call failed"));
    assert!(result.value.is_none());
}

#[test]
fn timeout_kills_the_kernel() {
    let mut req = python("import time\ntime.sleep(10)\n");
    req.timeout = Duration::from_millis(200);
    let result = run(&req).expect("run");
    assert_eq!(result.status, RunStatus::TimedOut);
    assert_eq!(result.exit_code, None);
    assert!(
        result.duration < Duration::from_secs(5),
        "timeout took {:?}",
        result.duration
    );
}

#[test]
fn call_reports_timeout_as_an_error() {
    let mut req = python("import time\ntime.sleep(10)\n");
    req.timeout = Duration::from_millis(200);
    match call(&req) {
        Err(ForeignError::Timeout { seconds }) => assert!((seconds - 0.2).abs() < 1e-6),
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[test]
fn call_rejects_stdout_noise() {
    let source = r#"print("debug log line")
print('{"type": "int", "decimal": "42"}')
"#;
    match call(&python(source)) {
        Err(ForeignError::Protocol { .. }) => {}
        other => panic!("expected Protocol error, got {other:?}"),
    }
}

#[test]
fn call_rejects_non_json_stdout() {
    match call(&python("print('not a wire value')")) {
        Err(ForeignError::Protocol { .. }) => {}
        other => panic!("expected Protocol error, got {other:?}"),
    }
}

#[test]
fn call_rejects_unknown_wire_types() {
    let source = r#"print('{"type": "mystery"}')"#;
    match call(&python(source)) {
        Err(ForeignError::Protocol { .. }) => {}
        other => panic!("expected Protocol error, got {other:?}"),
    }
}

#[test]
fn oversized_output_is_an_error() {
    let source = "print('x' * 200000)";
    let mut req = python(source);
    req.max_output_bytes = 1024;
    match run(&req) {
        Err(ForeignError::OutputTooLarge { limit }) => assert_eq!(limit, 1024),
        other => panic!("expected OutputTooLarge, got {other:?}"),
    }
}

#[test]
fn oversized_stderr_is_an_error() {
    let source = r#"import sys
sys.stderr.write("y" * 200000)
"#;
    let mut req = python(source);
    req.max_output_bytes = 512;
    match run(&req) {
        Err(ForeignError::OutputTooLarge { limit }) => assert_eq!(limit, 512),
        other => panic!("expected OutputTooLarge, got {other:?}"),
    }
}

#[test]
#[cfg(unix)]
fn child_environment_only_keeps_path() {
    let source = r#"import json, os
print(json.dumps(sorted(os.environ)))
"#;
    let result = run(&python(source)).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    let keys: Vec<String> = serde_json::from_str(&result.stdout).expect("json output");
    assert!(
        keys.iter().any(|key| key == "PATH"),
        "PATH must be preserved for the kernel: {keys:?}"
    );
    assert!(
        !keys.iter().any(|key| key == "HOME"),
        "the child must not inherit the parent environment: {keys:?}"
    );
}

#[test]
fn relative_writes_happen_in_a_fresh_cleaned_directory() {
    let source = r#"import os
with open("marqdo-foreign-relative-test.txt", "w", encoding="utf-8") as handle:
    handle.write("x")
print(os.getcwd())
"#;
    let result = run(&python(source)).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    let working_directory = result.stdout.trim();
    assert!(!working_directory.is_empty());
    assert!(
        !working_directory.starts_with(env!("CARGO_MANIFEST_DIR")),
        "foreign cwd must not be the project: {working_directory}"
    );
    assert!(
        !std::path::Path::new(working_directory).exists(),
        "the temporary working directory must be removed: {working_directory}"
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("marqdo-foreign-relative-test.txt")
            .exists(),
        "relative writes must not touch the project"
    );
}

#[test]
fn kernel_that_ignores_stdin_does_not_hang() {
    let mut req = python("print('done')");
    req.input = Some(json!({"blob": "x".repeat(4 * 1024 * 1024)}));
    let result = run(&req).expect("run");
    assert_eq!(
        result.status,
        RunStatus::Success,
        "stderr: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "done");
}

#[test]
fn unknown_language_is_unavailable() {
    match run(&request("cobol", "DISPLAY 'HI'.")) {
        Err(ForeignError::KernelUnavailable { language, hint }) => {
            assert_eq!(language, "cobol");
            assert!(!hint.is_empty());
        }
        other => panic!("expected KernelUnavailable, got {other:?}"),
    }
}

#[test]
fn error_messages_are_human_readable() {
    let error = ForeignError::OutputTooLarge { limit: 4096 };
    assert!(error.to_string().contains("4096"));
    let error = ForeignError::KernelUnavailable {
        language: "python".to_string(),
        hint: "install it".to_string(),
    };
    assert!(error.to_string().contains("python"));
    assert!(error.to_string().contains("install it"));
}
