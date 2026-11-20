//! Package instance identity, transitive imports, pinned Git and environment
//! materialization (M0.6 R1/R5).
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
            "marqdo-package-test-{}-{id}-{tag}",
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

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_marqdo")
}

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run marqdo")
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn transitive_path_dependency_resolves_from_its_own_manifest() {
    let dir = TempDir::new("transitive");
    let root = dir.path();
    write(
        &root.join("deps/a/marqdo.yaml"),
        "project:\n  name: a\n  version: 0.1.0\ndependencies:\n  b:\n    path: ./b\n",
    );
    write(
        &root.join("deps/a/index.mq.md"),
        "---\nimport:\n  b: \"@b\"\n---\n\n## outer\n\n*b.inner()*\n",
    );
    write(
        &root.join("deps/a/b/marqdo.yaml"),
        "project:\n  name: b\n  version: 0.1.0\n",
    );
    write(&root.join("deps/a/b/index.mq.md"), "## inner\n\n*42*\n");
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  a:\n    path: ./deps/a\n",
    );
    write(
        &root.join("index.mq.md"),
        "---\nimport:\n  a: \"@a\"\n---\n\n## main\n\n> 打印(a.outer())\n",
    );
    let output = cli(root, &["run", "index.mq.md"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
    assert!(
        !root.join(".marqdo").exists(),
        "run must not materialize env"
    );
}

#[test]
fn pinned_git_dependency_locks_syncs_and_runs() {
    if !have_git() {
        eprintln!("skipping: git is unavailable");
        return;
    }
    let dir = TempDir::new("git");
    let root = dir.path().join("project");
    let dep = dir.path().join("dep");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&dep).unwrap();
    write(
        &dep.join("marqdo.yaml"),
        "project:\n  name: remote\n  version: 0.3.0\n",
    );
    write(&dep.join("index.mq.md"), "## hello\n\n*7*\n");
    git(&dep, &["init", "-q"]);
    git(&dep, &["add", "."]);
    git(&dep, &["commit", "-q", "-m", "initial"]);
    let rev = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dep)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    write(
        &root.join("marqdo.yaml"),
        &format!(
            "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  remote:\n    git: {}\n    rev: {}\n",
            dep.display(),
            rev.trim()
        ),
    );
    write(
        &root.join("index.mq.md"),
        "---\nimport:\n  remote: \"@remote\"\n---\n\n## main\n\n> 打印(remote.hello())\n",
    );

    let lock = cli(&root, &["lock"]);
    assert_eq!(
        lock.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    let lock_text = std::fs::read_to_string(root.join("marqdo.lock")).unwrap();
    assert!(lock_text.contains("lock_version: 2"));
    assert!(lock_text.contains("instance_id: dep:remote@0.3.0#"));
    assert!(lock_text.contains("source: git:"));

    let sync = cli(&root, &["sync", "--locked"]);
    assert_eq!(
        sync.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let env_json = std::fs::read_to_string(root.join(".marqdo/env/env.json")).unwrap();
    assert!(env_json.contains("dep:remote@0.3.0#"));
    let run = cli(&root, &["run", "index.mq.md"]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "7\n");
}

#[test]
fn check_does_not_fetch_a_missing_git_revision() {
    if !have_git() {
        eprintln!("skipping: git is unavailable");
        return;
    }
    let dir = TempDir::new("git-missing");
    let root = dir.path();
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  remote:\n    git: /nonexistent/repo\n    rev: 0000000000000000000000000000000000000000\n",
    );
    write(
        &root.join("index.mq.md"),
        "---\nimport:\n  remote: \"@remote\"\n---\n\n## main\n\n*1*\n",
    );
    let output = cli(root, &["check", "index.mq.md"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not cached") || stderr.contains("cannot open dependency"));
    assert!(!root.join(".marqdo/cache/git").exists());
}
