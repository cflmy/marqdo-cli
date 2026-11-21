//! Isolated plugin worker transport (ADR-0010).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use marqdo_plugin_abi::{sha256_file, HostTarget};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-worker-test-{}-{id}-{tag}",
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

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Platform dynamic-library name for the test fixture.
fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "solver.dylib"
    } else {
        "solver.so"
    }
}

/// Flags for building a shared library with the platform C compiler.
fn shared_flags() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["-dynamiclib", "-fPIC"]
    } else {
        &["-shared", "-fPIC"]
    }
}

fn have_cc() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn compile(dir: &Path, extra: &[&str]) -> PathBuf {
    let output = dir.join(library_name());
    let status = Command::new("cc")
        .args(["-std=c11", "-O2"])
        .args(shared_flags())
        .arg("-I")
        .arg(repo().join("include"))
        .args(extra)
        .arg(repo().join("examples/plugin-demo/solver.c"))
        .arg("-o")
        .arg(&output)
        .status()
        .expect("cc");
    assert!(status.success(), "cc failed");
    output
}

fn write_manifest(dir: &Path, sha: &str) {
    std::fs::write(
        dir.join("plugin.yaml"),
        format!(
            r#"plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: worker
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: {}
      file: {}
      sha256: "{}"
"#,
            HostTarget::current().triple(),
            library_name(),
            sha
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("doc.mq.md"),
        "## @exec main\n\n`结果 = solver.add(a=20, b=22)`\n\n`打印(结果)`\n",
    )
    .unwrap();
}

fn run(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            dir.join("doc.mq.md").to_str().unwrap(),
            "--plugin",
            dir.to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
        ])
        .output()
        .unwrap()
}

#[test]
fn worker_transport_runs_without_in_process_trust() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("ok");
    let artifact = compile(dir.path(), &[]);
    write_manifest(dir.path(), &sha256_file(&artifact).unwrap());
    let output = run(dir.path());
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

#[test]
fn worker_crash_is_isolated_from_the_host() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("crash");
    let artifact = compile(dir.path(), &["-DMQ_SOLVER_CRASH"]);
    write_manifest(dir.path(), &sha256_file(&artifact).unwrap());
    let output = run(dir.path());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plugin.crashed") || stderr.contains("plugin.failure"),
        "{stderr}"
    );
    // The host process survived and still runs a native Marqdo program.
    let survivor = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run", "examples/hello.mq.md"])
        .current_dir(repo())
        .output()
        .unwrap();
    assert_eq!(survivor.status.code(), Some(0));
}

fn compile_capability(dir: &Path, extra: &[&str]) -> PathBuf {
    let output = dir.join(if cfg!(target_os = "macos") {
        "capability.dylib"
    } else {
        "capability.so"
    });
    let status = Command::new("cc")
        .args(["-std=c11", "-O2"])
        .args(shared_flags())
        .arg("-I")
        .arg(repo().join("include"))
        .args(extra)
        .arg(repo().join("tests/abi/capability_plugin.c"))
        .arg("-o")
        .arg(&output)
        .status()
        .expect("cc");
    assert!(status.success(), "cc failed");
    output
}

fn write_capability_manifest(dir: &Path, sha: &str) {
    std::fs::write(
        dir.join("plugin.yaml"),
        format!(
            r#"plugin:
  name: demo.capability
  version: 0.1.0
  abi: "^1.0"
  kind: worker
  exports:
    - name: capability.read
      signature: "() -> Text"
  requires:
    - fs.read
  artifacts:
    - target: {}
      file: {}
      sha256: "{}"
"#,
            HostTarget::current().triple(),
            if cfg!(target_os = "macos") {
                "capability.dylib"
            } else {
                "capability.so"
            },
            sha
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("doc.mq.md"),
        "## @exec main\n\n`结果 = capability.read()`\n\n`打印(结果)`\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join("data/allowed.txt"), "hello-worker").unwrap();
}

fn run_capability(dir: &Path, allow_read: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_marqdo"));
    command.args([
        "run",
        dir.join("doc.mq.md").to_str().unwrap(),
        "--plugin",
        dir.to_str().unwrap(),
        "--allow-plugin",
        "demo.capability",
    ]);
    if allow_read {
        command.args(["--allow-read", "data/**"]);
    }
    command.output().unwrap()
}

#[test]
fn worker_capability_bridge_reads_granted_file_and_denies_without_grant() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("capability");
    let artifact = compile_capability(dir.path(), &[]);
    write_capability_manifest(dir.path(), &sha256_file(&artifact).unwrap());

    let output = run_capability(dir.path(), true);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello-worker\n");

    let denied = run_capability(dir.path(), false);
    assert_eq!(denied.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        stderr.contains("permission.denied") || stderr.contains("fs.read"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&denied.stdout).contains("hello-worker"));
}

#[test]
fn worker_capability_request_budget_stops_a_runaway_plugin() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("capability-budget");
    let artifact = compile_capability(dir.path(), &["-DMQ_CAPABILITY_LOOP"]);
    write_capability_manifest(dir.path(), &sha256_file(&artifact).unwrap());
    let output = run_capability(dir.path(), true);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("capability.budget_exceeded"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("hello-worker"));
}
