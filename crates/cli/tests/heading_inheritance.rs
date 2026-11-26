//! Black-box regression tests for RFC-EXT-0003 H-06 sibling heading modes.
//!
//! A heading that does not declare `@exec`/`@knowledge` inherits its nearest
//! *ancestor*, never the previous sibling. These tests drive the real `marqdo`
//! binary and assert that `check --emit`, `run` and `call` agree.

#![cfg(unix)]
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-heading-test-{}-{id}-{tag}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&str], dir: &Path) -> Output {
    stdlib_support::cli_command()
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn units(document: &serde_json::Value) -> Vec<&serde_json::Value> {
    document["modules"][0]["units"]
        .as_array()
        .expect("units array")
        .iter()
        .collect()
}

fn unit<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    units(document)
        .into_iter()
        .find(|unit| unit["name"] == name)
        .unwrap_or_else(|| panic!("missing unit {name}"))
}

fn exec_document() -> &'static str {
    "# @knowledge 数学\n\n## @exec main\n\n`return 1`\n\n## 普通说明\n\n这段是知识叙述。\n"
}

fn knowledge_document() -> &'static str {
    "# @exec 项目\n\n## @knowledge 解释\n\n背景。\n\n## main\n\n`return 2`\n"
}

#[test]
fn check_emit_marks_unmarked_sibling_knowledge() {
    let dir = TempDir::new("emit-knowledge");
    dir.write("index.mq.md", exec_document());
    let analysis = dir.path().join("analysis.json");
    let check = cli(
        &["check", "index.mq.md", "--emit", analysis.to_str().unwrap()],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&analysis).unwrap()).unwrap();
    assert_eq!(unit(&document, "数学")["kind"], "namespace");
    assert_eq!(unit(&document, "main")["kind"], "function");
    assert_eq!(unit(&document, "普通说明")["kind"], "knowledge");
    assert!(unit(&document, "普通说明")["function"].is_null());
    // The analysis exposes exactly one function and it is the Exec sibling.
    let functions = document["modules"][0]["functions"].as_array().unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["local_name"], "main");
    assert_eq!(functions[0]["namespace"], "数学");
    assert_eq!(functions[0]["unit_id"], unit(&document, "main")["id"]);
}

#[test]
fn check_emit_marks_unmarked_sibling_exec() {
    let dir = TempDir::new("emit-exec");
    dir.write("index.mq.md", knowledge_document());
    let analysis = dir.path().join("analysis.json");
    let check = cli(
        &["check", "index.mq.md", "--emit", analysis.to_str().unwrap()],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(analysis).unwrap()).unwrap();
    assert_eq!(unit(&document, "解释")["kind"], "knowledge");
    assert_eq!(unit(&document, "main")["kind"], "function");
    let functions = document["modules"][0]["functions"].as_array().unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["local_name"], "main");
    assert_eq!(functions[0]["namespace"], "项目");
    assert_eq!(functions[0]["unit_id"], unit(&document, "main")["id"]);
}

#[test]
fn run_and_call_not_callable_knowledge_sibling() {
    let dir = TempDir::new("call");
    dir.write("index.mq.md", exec_document());
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // `run` streams program stdout only; the returned value is reported by
    // `call`, whose output is asserted below and must agree.
    assert!(stdout(&run).is_empty());

    let call = cli(&["call", "index.mq.md", "数学.main"], dir.path());
    assert_eq!(call.status.code(), Some(0), "{}", stderr(&call));
    assert_eq!(stdout(&call), "1\n");

    let knowledge_sibling = cli(&["call", "index.mq.md", "数学.普通说明"], dir.path());
    assert_eq!(knowledge_sibling.status.code(), Some(1));
    assert!(
        stderr(&knowledge_sibling).contains("sem.no_entry_function"),
        "{}",
        stderr(&knowledge_sibling)
    );
    assert!(stdout(&knowledge_sibling).is_empty());
}

#[test]
fn exec_function_run_and_call_agree() {
    let dir = TempDir::new("exec");
    dir.write("index.mq.md", knowledge_document());
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stdout(&run).is_empty());

    let call = cli(&["call", "index.mq.md", "项目.main"], dir.path());
    assert_eq!(call.status.code(), Some(0), "{}", stderr(&call));
    assert_eq!(stdout(&call), "2\n");

    let knowledge = cli(&["call", "index.mq.md", "项目.解释"], dir.path());
    assert_eq!(knowledge.status.code(), Some(1));
    assert!(stderr(&knowledge).contains("sem.no_entry_function"));
}
