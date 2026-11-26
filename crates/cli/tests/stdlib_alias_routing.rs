//! Official stdlib public alias routing (RFC-EXT-0001 §8, ADR-0058).
//!
//! `打印`/`print` are ordinary `Marqdo` bindings to `io.mq.md::写值` declared
//! in the verified `stdlib.json` metadata. There is no `print_override`, no
//! runtime name special case and no hard-coded per-function scan.
#![cfg(unix)]

#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-stdlib-alias-test-{}-{id}-{tag}",
            std::process::id()
        ));
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

fn cli(args: &[&str], dir: &Path, env: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_marqdo"));
    command.args(args).current_dir(dir);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn copy_stdlib(target: &Path) {
    let source = stdlib_support::bundle_dir();
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &target.join(entry.file_name()));
        } else {
            std::fs::copy(&path, target.join(entry.file_name())).unwrap();
        }
    }
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &target.join(entry.file_name()));
        } else {
            std::fs::copy(&path, target.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn print_and_its_alias_are_bound_to_the_official_module() {
    let dir = TempDir::new("print");
    std::fs::write(
        dir.path().join("index.mq.md"),
        "## @exec main\n\n`打印(1)`\n\n`print(\"two\")`\n\n`return None`\n",
    )
    .unwrap();
    let output = cli(&["run", "index.mq.md"], dir.path(), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "1\ntwo\n");
}

#[test]
fn program_without_print_does_not_load_the_stdlib_module_graph() {
    let dir = TempDir::new("no-print");
    std::fs::write(
        dir.path().join("index.mq.md"),
        "## @exec main\n\n`params()`\n\n`return 1 + 2`\n",
    )
    .unwrap();
    let analysis_path = dir.path().join("factorial.json");
    let output = cli(
        &[
            "check",
            "index.mq.md",
            "--emit",
            analysis_path.to_str().unwrap(),
        ],
        dir.path(),
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let analysis: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&analysis_path).unwrap()).unwrap();
    let modules = analysis["modules"].as_array().unwrap();
    assert!(
        modules
            .iter()
            .all(|module| !module["id"].as_str().unwrap_or("").starts_with("std:")),
        "a program without print must not load the stdlib: {analysis}"
    );
}

#[test]
fn print_program_loads_exactly_the_declared_alias_target() {
    let dir = TempDir::new("with-print");
    std::fs::write(
        dir.path().join("index.mq.md"),
        "## @exec main\n\n`打印(\"x\")`\n\n`return None`\n",
    )
    .unwrap();
    let analysis_path = dir.path().join("hello.json");
    let output = cli(
        &[
            "check",
            "index.mq.md",
            "--emit",
            analysis_path.to_str().unwrap(),
        ],
        dir.path(),
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let analysis: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&analysis_path).unwrap()).unwrap();
    let ids: Vec<&str> = analysis["modules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|module| module["id"].as_str())
        .collect();
    assert!(
        ids.iter().any(|id| id.ends_with("::io.mq.md")),
        "print must load io.mq.md: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| id.ends_with("::format.mq.md")),
        "io.mq.md imports format.mq.md: {ids:?}"
    );
}

#[test]
fn tampered_public_alias_metadata_digest_is_rejected() {
    let dir = TempDir::new("alias-digest");
    let stdlib = dir.path().join("stdlib");
    copy_stdlib(&stdlib);
    let manifest_path = stdlib.join("stdlib.json");
    let text = std::fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    manifest["aliases_digest"] = serde_json::json!("sha256:".to_string() + &"0".repeat(64));
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .unwrap();
    let doc = dir.path().join("index.mq.md");
    std::fs::write(&doc, "## @exec main\n\n`打印(1)`\n\n`return None`\n").unwrap();
    let output = cli(
        &["run", doc.to_str().unwrap()],
        dir.path(),
        &[("MARQDO_STDLIB_DIR", &stdlib)],
    );
    assert!(!output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("public-alias metadata digest mismatch"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn tampered_alias_target_signature_is_rejected() {
    let dir = TempDir::new("alias-target");
    let stdlib = dir.path().join("stdlib");
    copy_stdlib(&stdlib);
    let manifest_path = stdlib.join("stdlib.json");
    let text = std::fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    manifest["public_aliases"][0]["expected_signature"] = serde_json::json!("params(值, 额外)");
    // Recompute the declared metadata digest so only the target check can fail.
    let canonical = manifest["public_aliases"][0].clone();
    let module = canonical["target_module"].as_str().unwrap();
    let function = canonical["target_function"].as_str().unwrap();
    let mut names: Vec<&str> = canonical["public_names"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    names.sort();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(module.as_bytes());
    hasher.update([0]);
    hasher.update(function.as_bytes());
    hasher.update([0]);
    for name in names {
        hasher.update(name.as_bytes());
        hasher.update([0]);
    }
    let digest = format!("sha256:{}", hex(&hasher.finalize()));
    manifest["aliases_digest"] = serde_json::json!(digest);
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .unwrap();
    let doc = dir.path().join("index.mq.md");
    std::fs::write(&doc, "## @exec main\n\n`打印(1)`\n\n`return None`\n").unwrap();
    let output = cli(
        &["run", doc.to_str().unwrap()],
        dir.path(),
        &[("MARQDO_STDLIB_DIR", &stdlib)],
    );
    assert!(!output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("expected signature") || stderr(&output).contains("alias"),
        "{}",
        stderr(&output)
    );
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
