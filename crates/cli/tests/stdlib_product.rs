//! CLI product lane against the accepted external stdlib Bundle (ADR-0070 Phase B).
//!
//! The standard-library module sources and their self-tests live in the
//! component repositories; this lane proves what the CLI still owns: it loads
//! one verified offline Bundle, resolves the public `std:` surface, and routes
//! the official `print`/`打印` aliases through the ordinary target function.
//! The Bundle is an explicit input (`MARQDO_TEST_STDLIB_BUNDLE`) verified
//! against `tools/stdlib/bundle-pin.json`; there is no in-repo fallback.

#[path = "support/observation.rs"]
pub mod observation;

#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::process::{Command, Output};

fn run_fixture(name: &str) -> (serde_json::Value, String) {
    let bundle = stdlib_support::bundle_dir();
    let fixture = stdlib_support::repo_root()
        .join("tests")
        .join("product-stdlib")
        .join(format!("{name}.mq.md"));
    let output: Output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            fixture.to_str().expect("utf-8 fixture path"),
            "--output",
            "json",
        ])
        .env("MARQDO_STDLIB_DIR", &bundle)
        .current_dir(stdlib_support::repo_root())
        .output()
        .expect("run marqdo");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json report");
    assert_eq!(report["run"]["status"], "ok", "{name}");
    let stdout = observation::text(&report, "stdout.chunk");
    (report, stdout)
}

#[test]
fn the_accepted_bundle_is_external_and_has_the_required_surface() {
    let pin = stdlib_support::BundlePin::committed();
    assert!(
        pin.source_path.is_none(),
        "Phase B: the accepted Bundle must not be vendored in this repository"
    );
    let bundle = stdlib_support::bundle_dir();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("stdlib.json")).unwrap()).unwrap();
    assert_eq!(manifest["name"], pin.name);
    assert_eq!(manifest["version"], pin.version);
    assert_eq!(manifest["digest"], pin.tree_digest);
    assert_eq!(manifest["aliases_digest"], pin.aliases_digest);
    for required in [
        "io.mq.md",
        "io/console.mq.md",
        "io/stream.mq.md",
        "format.mq.md",
        "text.mq.md",
        "text/utf8.mq.md",
        "data/json.mq.md",
        "collections/table.mq.md",
        "data/table.mq.md",
        "fs/file.mq.md",
        "path.mq.md",
    ] {
        assert!(
            bundle.join(required).is_file(),
            "the accepted Bundle is missing {required}"
        );
    }
    assert!(
        !bundle.join("tests").exists(),
        "the accepted Bundle must not ship component self-tests"
    );
}

#[test]
fn official_print_aliases_match_the_target_function_output() {
    let (report, stdout) = run_fixture("print-routing");
    assert_eq!(report["run"]["status"], "ok");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 6, "{stdout:?}");
    for pair in [(0, 1), (2, 3), (4, 5)] {
        assert_eq!(
            lines[pair.0], lines[pair.1],
            "official alias formatting differs at line {}",
            pair.0
        );
    }
    assert_eq!(
        lines[0],
        r#"["文本", 2, 1.5, [3, true], {a: 1, b: "x"}, None]"#
    );
    assert_eq!(lines[2], "Table[名称 | 分数]{甲 | 87; 乙 | 92}");
    assert_eq!(
        lines[4],
        r#"[Formula("a = 1"), Code("text", 9 bytes), FunctionRef(root:default::print-routing.mq.md::演示函数)]"#
    );
}

#[test]
fn std_surface_executes_against_the_official_bundle() {
    let (report, stdout) = run_fixture("std-surface");
    assert_eq!(stdout, "");
    assert_eq!(
        observation::terminal(&report)["value"],
        serde_json::json!({
            "type": "list",
            "items": [
                {"type": "int", "decimal": "2"},
                {"type": "text", "value": "é"},
                {"type": "text", "value": "[1, 2]"},
                {"type": "text", "value": "[\"ok\", 3]"},
                {"type": "text", "value": "a/b"},
                {"type": "text", "value": "2026-11-21T08:00:00.123Z"},
                {"type": "text", "value": "| 名称 | 分数 |\n| --- | --- |\n| 甲 | 87 |\n| 乙 | 92 |"}
            ]
        })
    );
}

#[test]
fn a_missing_explicit_bundle_fails_closed_without_fallback() {
    let missing = stdlib_support::repo_root()
        .join("target")
        .join("does-not-exist-stdlib");
    let fixture = stdlib_support::repo_root().join("tests/product-stdlib/std-surface.mq.md");
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            fixture.to_str().expect("utf-8 fixture path"),
            "--output",
            "json",
        ])
        .env("MARQDO_STDLIB_DIR", &missing)
        .current_dir(stdlib_support::repo_root())
        .output()
        .expect("run marqdo");
    assert_ne!(
        output.status.code(),
        Some(0),
        "an invalid explicit MARQDO_STDLIB_DIR must fail even when a Bundle is installed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stdlib") || stderr.contains("cannot open"),
        "failure must name the Bundle: {stderr}"
    );
}
