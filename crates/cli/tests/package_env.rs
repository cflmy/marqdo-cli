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
    let lock = cli(root, &["lock"]);
    assert_eq!(
        lock.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
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
    assert!(lock_text.contains("lock_version: 3"));
    assert!(lock_text.contains("root_dependencies:"));
    assert!(lock_text.contains("instance_id: dep:remote@0.3.0#"));
    assert!(lock_text.contains("kind: git"));
    assert!(lock_text.contains("content_digest: "));

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
    assert!(stderr.contains("marqdo.lock is missing"), "{stderr}");
    assert!(!root.join(".marqdo/cache/git").exists());
}

#[test]
fn run_and_check_require_a_lock_when_dependencies_exist() {
    let dir = TempDir::new("no-lock");
    let root = dir.path();
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  dep:\n    path: ./dep\n",
    );
    write(&root.join("dep/index.mq.md"), "## value\n\n*1*\n");
    write(
        &root.join("index.mq.md"),
        "---\nimport:\n  dep: \"@dep\"\n---\n\n## main\n\n*1*\n",
    );
    for command in ["check", "run"] {
        let output = cli(root, &[command, "index.mq.md"]);
        assert_eq!(output.status.code(), Some(1), "{command}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("marqdo.lock is missing"), "{stderr}");
    }
}

#[test]
fn sync_is_strict_and_never_rewrites_the_lock() {
    let dir = TempDir::new("strict-sync");
    let root = dir.path();
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  dep:\n    path: ./dep\n",
    );
    write(&root.join("dep/index.mq.md"), "## value\n\n*1*\n");
    write(&root.join("index.mq.md"), "## main\n\n*1*\n");

    let lock = cli(root, &["lock"]);
    assert_eq!(lock.status.code(), Some(0));
    let original = std::fs::read(root.join("marqdo.lock")).unwrap();

    write(&root.join("dep/index.mq.md"), "## value\n\n*2*\n");
    let stale = cli(root, &["sync"]);
    assert_eq!(stale.status.code(), Some(1), "plain sync must be strict");
    let stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(stderr.contains("lockfile is out of date"), "{stderr}");
    assert_eq!(
        std::fs::read(root.join("marqdo.lock")).unwrap(),
        original,
        "sync must never rewrite the lock"
    );

    let relock = cli(root, &["lock"]);
    assert_eq!(relock.status.code(), Some(0));
    let after_relock = std::fs::read(root.join("marqdo.lock")).unwrap();
    let synced = cli(root, &["sync"]);
    assert_eq!(synced.status.code(), Some(0));
    assert_eq!(
        std::fs::read(root.join("marqdo.lock")).unwrap(),
        after_relock
    );
}

#[test]
fn lock_records_transitive_closure_and_sync_materializes_it() {
    let dir = TempDir::new("closure");
    let root = dir.path();
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  a:\n    path: ./deps/a\n",
    );
    write(
        &root.join("deps/a/marqdo.yaml"),
        "project:\n  name: a\n  version: 0.1.0\ndependencies:\n  b:\n    path: ./b\n",
    );
    write(&root.join("deps/a/index.mq.md"), "## a\n\n*1*\n");
    write(
        &root.join("deps/a/b/marqdo.yaml"),
        "project:\n  name: b\n  version: 0.1.0\n",
    );
    write(&root.join("deps/a/b/index.mq.md"), "## b\n\n*2*\n");
    write(&root.join("index.mq.md"), "## main\n\n*1*\n");

    let lock = cli(root, &["lock"]);
    assert_eq!(lock.status.code(), Some(0));
    let lock_text = std::fs::read_to_string(root.join("marqdo.lock")).unwrap();
    assert!(lock_text.contains("name: a"), "{lock_text}");
    assert!(lock_text.contains("name: b"), "{lock_text}");
    assert!(lock_text.contains("b: dep:b"), "{lock_text}");

    let sync = cli(root, &["sync", "--locked"]);
    assert_eq!(
        sync.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let env: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".marqdo/env/env.json")).unwrap())
            .unwrap();
    assert_eq!(env["schema_version"], 2);
    assert_eq!(
        env["packages"].as_array().map(|packages| packages.len()),
        Some(2)
    );
    assert!(env["lock_digest"].as_str().unwrap().len() == 64);
}

#[test]
fn deleted_lock_edge_fails_check_and_run() {
    let dir = TempDir::new("missing-edge");
    let root = dir.path();
    write(
        &root.join("marqdo.yaml"),
        "project:\n  name: root\n  version: 0.1.0\ndependencies:\n  dep:\n    path: ./dep\n",
    );
    write(&root.join("dep/index.mq.md"), "## value\n\n*1*\n");
    write(
        &root.join("index.mq.md"),
        "---\nimport:\n  dep: \"@dep\"\n---\n\n## main\n\n*1*\n",
    );
    assert_eq!(cli(root, &["lock"]).status.code(), Some(0));
    let lock = std::fs::read_to_string(root.join("marqdo.lock")).unwrap();
    let head = &lock[..lock.find("root_dependencies:").unwrap()];
    write(
        &root.join("marqdo.lock"),
        &format!("{head}root_dependencies: {{}}\n"),
    );
    for command in ["check", "run"] {
        let output = cli(root, &[command, "index.mq.md"]);
        assert_eq!(output.status.code(), Some(1), "{command}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not recorded in marqdo.lock")
                || stderr.contains("not reachable from root_dependencies"),
            "{stderr}"
        );
    }
}
