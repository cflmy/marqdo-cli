//! R6 integration examples that do not require an official external repo.
#![cfg(unix)]

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
    for name in ["analysis-v1", "trace-v1", "tool-v1"] {
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
