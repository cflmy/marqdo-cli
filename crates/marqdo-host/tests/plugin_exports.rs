//! Complete-name plugin export routing (ADR-0010).
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use marqdo_host::plugin::{sha256_file, HostTarget, PluginManifest};
use marqdo_host::{Capabilities, StdHost};
use marqdo_host_contract::{Host, HostOp};
use marqdo_sandbox::SandboxMode;
use marqdo_value::Value;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("marqdo-host-plugin-{}-{id}", std::process::id()));
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn have_cc() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_plugin(dir: &Path) -> PathBuf {
    let output = dir.join("solver.so");
    let status = Command::new("cc")
        .args(["-std=c11", "-O2", "-shared", "-fPIC"])
        .arg("-I")
        .arg(repo_root().join("include"))
        .arg("-DMQ_SOLVER_MULTI_EXPORT")
        .arg(repo_root().join("examples/plugin-demo/solver.c"))
        .arg("-o")
        .arg(&output)
        .status()
        .expect("spawn cc");
    assert!(status.success(), "cc failed");
    output
}

fn write_manifest(dir: &Path, sha: &str) {
    let yaml = format!(
        r#"plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
    - name: solver.neg
      signature: "($a: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: {}
      file: solver.so
      sha256: "{}"
"#,
        HostTarget::current().triple(),
        sha
    );
    std::fs::write(dir.join("plugin.yaml"), yaml).unwrap();
}

#[test]
fn every_export_is_registered_by_complete_name_and_conflicts_are_atomic() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new();
    let artifact = build_plugin(dir.path());
    write_manifest(dir.path(), &sha256_file(&artifact).unwrap());
    let manifest = PluginManifest::load(&dir.path().join("plugin.yaml")).unwrap();

    let capabilities = Capabilities::project().allow_plugin("demo.solver");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let names = host.load_plugin(&manifest, dir.path(), true).unwrap();
    assert_eq!(names, vec!["solver.add", "solver.neg"]);
    assert_eq!(host.plugin_exports().len(), 2);

    let ctx = host.context("root:default::test.mq.md", None);
    let mut args = BTreeMap::new();
    args.insert("a".to_string(), Value::Int(7.into()));
    let value = host
        .invoke(
            HostOp::PluginInvoke,
            vec![Value::Text("solver.neg".into()), Value::Map(args.clone())],
            &ctx,
        )
        .unwrap();
    assert_eq!(value, Value::Int((-7).into()));

    let mut add_args = BTreeMap::new();
    add_args.insert("a".to_string(), Value::Int(7.into()));
    add_args.insert("b".to_string(), Value::Int(0.into()));
    let value = host
        .invoke(
            HostOp::PluginInvoke,
            vec![Value::Text("solver.add".into()), Value::Map(add_args)],
            &ctx,
        )
        .unwrap();
    assert_eq!(value, Value::Int(7.into()));

    let duplicate = host.load_plugin(&manifest, dir.path(), true).unwrap_err();
    assert_eq!(
        duplicate.category,
        marqdo_value::ErrorCategory::PluginFailure
    );
    assert_eq!(host.plugin_exports().len(), 2);
}
