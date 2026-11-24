//! R6 integration examples that do not require an official external repo.
#![cfg(unix)]

#[path = "support/observation.rs"]
pub mod observation;

use std::path::Path;
use std::process::{Command, Output};

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(args)
        .current_dir(repo())
        .output()
        .expect("run marqdo")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn mqmd_stdlib_print_output_matches_builtin_print() {
    let output = cli(&["run", "stdlib/demo.mq.md"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 6, "{stdout:?}");
    for pair in [(0, 1), (2, 3), (4, 5)] {
        assert_eq!(
            lines[pair.0], lines[pair.1],
            "stdlib formatting differs from 打印 at line {}",
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
        r#"[Formula("a = 1"), Code("text", 9 bytes), FunctionRef(root:default::demo.mq.md::演示函数)]"#
    );
}

#[test]
fn addressable_memory_example_runs_through_the_cli() {
    let output = cli(&["run", "examples/memory-buffer.mq.md", "--output", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("run --output json emits a snapshot");
    assert_eq!(report["run"]["status"], "ok");
    assert_eq!(observation::terminal(&report)["value"]["type"], "int");
    assert_eq!(observation::terminal(&report)["value"]["decimal"], "141");
}

#[test]
fn cli_memory_budget_override_fails_closed() {
    let output = cli(&[
        "run",
        "examples/memory-buffer.mq.md",
        "--max-memory",
        "0",
        "--output",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("run --output json emits a snapshot");
    assert_eq!(report["run"]["status"], "error");
    assert_eq!(
        observation::terminal(&report)["error"]["code"],
        "resource.exhausted"
    );
}

#[test]
fn multi_version_packages_run_without_an_implicit_override() {
    let output = cli(&["run", "examples/packages-multi-version/index.mq.md"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
}

#[test]
fn external_kernel_provider_runs_without_core_changes() {
    let output = cli(&[
        "run",
        "examples/kernel-mock/demo.mq.md",
        "--kernel",
        "examples/kernel-mock/kernel.json",
        "--allow-foreign",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

#[test]
fn unknown_tool_name_installs_and_dispatches() {
    let home = std::env::temp_dir().join(format!(
        "marqdo-r6-tool-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let install = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["tool", "install", "examples/tool-unknown-name/tool.json"])
        .env("MARQDO_TOOL_HOME", &home)
        .current_dir(repo())
        .output()
        .unwrap();
    assert_eq!(install.status.code(), Some(0), "{}", stderr(&install));
    let run = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["benchmark", "run", "."])
        .env("MARQDO_TOOL_HOME", &home)
        .current_dir(repo())
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(String::from_utf8_lossy(&run.stdout).contains("arg=run"));
    let remove = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["tool", "remove", "benchmark"])
        .env("MARQDO_TOOL_HOME", &home)
        .current_dir(repo())
        .output()
        .unwrap();
    assert_eq!(remove.status.code(), Some(0), "{}", stderr(&remove));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn public_schemas_are_valid_json_and_versioned() {
    for name in [
        "analysis",
        "observation-event",
        "cli-tool",
        "observation-file",
    ] {
        let path = repo()
            .join("spec/schemas")
            .join(format!("{name}.schema.json"));
        let schema: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert!(schema["title"].is_string());
        assert!(schema["type"].as_str() == Some("object"));
    }
}
