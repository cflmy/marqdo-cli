//! RFC-EXT-0003 H-01 / T-MATH-01..06: the core has no `math.*` domain
//! builtins; algorithms live in pure `.mq.md` packages and optionally in an
//! explicitly selected ABI backend. This test drives the real CLI binary and the
//! checked-in examples.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-math-test-{}-{id}-{tag}",
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

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn cli(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
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

#[test]
fn pure_math_library_checks_calls_and_runs_without_math_builtins() {
    let repo = repo();
    let check = cli(&["check", "examples/math-library-demo.mq.md"], &repo);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));

    let call = cli(&["call", "examples/math-library-demo.mq.md", "main"], &repo);
    assert_eq!(call.status.code(), Some(0), "{}", stderr(&call));
    assert_eq!(stdout(&call), "6\n");

    let run = cli(&["call", "examples/math.mq.md", "计算圆面积"], &repo);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
}

#[test]
fn removed_math_builtins_are_static_symbol_errors() {
    let dir = TempDir::new("removed");
    dir.write(
        "index.mq.md",
        "# @knowledge 公式\n\n$$\nA = r + 1\n$$\n\n## @exec main\n\n`return math.eval(ref(\"#公式\"), {\"r\": 2}, target=\"A\")`\n",
    );
    let check = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(check.status.code(), Some(1));
    let error = stderr(&check);
    assert!(error.contains("symbol.unknown"), "{error}");
    // No fallback execution and no removed `math.unsupported` runtime path.
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(1));
    assert!(run.stdout.is_empty());
    assert!(stderr(&run).contains("symbol.unknown"), "{}", stderr(&run));
}
