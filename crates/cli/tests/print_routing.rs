//! Compat-B `打印` routing tests (ADR-0030).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("marqdo-print-route-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run marqdo")
}

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

#[test]
fn print_is_routed_through_the_stdlib_and_not_hostop_print() {
    let dir = TempDir::new("routed");
    write(
        dir.path(),
        "index.mq.md",
        "## main\n\n> 打印([\"x\", 1])\n\n*None*\n",
    );
    let trace = dir.path().join("trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--trace-file",
            trace.to_str().unwrap(),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[\"x\", 1]\n");

    let trace: serde_json::Value = serde_json::from_slice(&std::fs::read(&trace).unwrap()).unwrap();
    let operations: Vec<&str> = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["operation"].as_str())
        .collect();
    assert!(
        operations.contains(&"resource.write"),
        "stdlib print must write through the resource ABI: {operations:?}"
    );
    assert!(
        !operations.contains(&"print"),
        "legacy HostOp::Print must not run in Compat-B: {operations:?}"
    );
}

#[test]
fn stdlib_print_modules_are_injected_only_when_print_is_used() {
    let dir = TempDir::new("lazy");
    write(dir.path(), "plain.mq.md", "## main\n\n*1*\n");
    write(
        dir.path(),
        "prints.mq.md",
        "## main\n\n> 打印(1)\n\n*None*\n",
    );
    for (file, expected_stdlib) in [("plain.mq.md", false), ("prints.mq.md", true)] {
        let target = dir.path().join(format!("{file}.json"));
        let output = cli(
            dir.path(),
            &["check", file, "--emit", target.to_str().unwrap()],
        );
        assert_eq!(output.status.code(), Some(0), "{file}");
        let analysis: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        let modules = analysis["modules"].as_array().unwrap();
        let has_stdlib = modules
            .iter()
            .any(|module| module["id"].as_str().unwrap_or("").starts_with("std:"));
        assert_eq!(has_stdlib, expected_stdlib, "{file}: {modules:?}");
        // The bundle identity is recorded whenever a bundle is available; it
        // does not imply that this program loaded any stdlib module.
        assert!(analysis["project"]["stdlib"].is_object(), "{file}");
    }
}
