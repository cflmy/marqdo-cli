//! Bundled stdlib (`std:`) and frozen-CLI closure tests (CM-A10).
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::path::{Path, PathBuf};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_marqdo")
}

fn stdlib_dir() -> PathBuf {
    stdlib_support::bundle_dir()
}

fn run(dir: &Path, stdlib: &Path, args: &[&str]) -> std::process::Output {
    stdlib_support::cli_command()
        .args(args)
        .current_dir(dir)
        .env("MARQDO_STDLIB_DIR", stdlib)
        .output()
        .expect("run marqdo")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("marqdo-stdlib-{}-{tag}", std::process::id()));
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

fn copy_stdlib(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_stdlib(&path, &target.join(entry.file_name()));
        } else {
            std::fs::copy(&path, target.join(entry.file_name())).unwrap();
        }
    }
}

fn write_project(dir: &Path, program: &str) {
    std::fs::write(dir.join("index.mq.md"), program).unwrap();
}

const IMPORT_IO: &str = r#"---
import:
  io: "std:io"
---

## @exec main

`io.写值(["bundle", 1, 2.5])`

`return None`
"#;

#[test]
fn std_import_resolves_the_verified_bundle() {
    let dir = TempDir::new("resolve");
    write_project(dir.path(), IMPORT_IO);
    let output = run(dir.path(), &stdlib_dir(), &["run", "index.mq.md"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[\"bundle\", 1, 2.5]\n"
    );
}

#[test]
fn analysis_records_the_verified_stdlib_digest() {
    let dir = TempDir::new("analysis");
    write_project(dir.path(), IMPORT_IO);
    let target = dir.path().join("analysis.json");
    let output = run(
        dir.path(),
        &stdlib_dir(),
        &["check", "index.mq.md", "--emit", target.to_str().unwrap()],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let analysis: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(stdlib_dir().join("stdlib.json")).unwrap()).unwrap();
    assert_eq!(
        analysis["project"]["stdlib"]["version"],
        manifest["version"]
    );
    assert_eq!(analysis["project"]["stdlib"]["digest"], manifest["digest"]);
    assert!(analysis["project"]["stdlib"]["instance_id"]
        .as_str()
        .unwrap()
        .starts_with("std:marqdo-stdlib@"));
}

#[test]
fn frozen_binary_hash_is_unchanged_after_stdlib_execution() {
    let before = std::fs::read(binary()).unwrap();
    let before_hash = marqdo_value_sha(&before);
    let dir = TempDir::new("frozen");
    write_project(dir.path(), IMPORT_IO);
    let output = run(dir.path(), &stdlib_dir(), &["run", "index.mq.md"]);
    assert_eq!(output.status.code(), Some(0));
    let after = std::fs::read(binary()).unwrap();
    assert_eq!(before_hash, marqdo_value_sha(&after));
}

fn marqdo_value_sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn tampered_bundle_is_rejected_before_module_loading() {
    let dir = TempDir::new("tamper");
    let bundle = dir.path().join("stdlib");
    copy_stdlib(&stdlib_dir(), &bundle);
    std::fs::write(bundle.join("io.mq.md"), "## @knowledge tampered\n").unwrap();
    write_project(dir.path(), IMPORT_IO);
    let output = run(dir.path(), &bundle, &["run", "index.mq.md"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stdlib digest mismatch"), "{stderr}");
}

#[test]
fn bundle_uses_no_foreign_or_plugin_calls() {
    fn visit(dir: &Path, offenders: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(&path, offenders);
            } else if path.to_string_lossy().ends_with(".mq.md") {
                let source = std::fs::read_to_string(&path).unwrap();
                if source.contains("foreign.") || source.contains("plugin.") {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    let mut offenders = Vec::new();
    visit(&stdlib_dir(), &mut offenders);
    assert!(
        offenders.is_empty(),
        "stdlib bundle must not depend on foreign/plugin calls: {offenders:?}"
    );
}

#[test]
fn missing_bundle_manifest_is_a_hard_error() {
    let dir = TempDir::new("missing");
    let empty = dir.path().join("empty-stdlib");
    std::fs::create_dir_all(&empty).unwrap();
    write_project(dir.path(), IMPORT_IO);
    let output = run(dir.path(), &empty, &["run", "index.mq.md"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stdlib"), "{stderr}");
}
