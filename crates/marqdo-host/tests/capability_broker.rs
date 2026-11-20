//! End-to-end native plugin capability execution (H3-A).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use marqdo_host::{Capabilities, Host, HostOp, StdHost};
use marqdo_plugin_abi::{sha256_file, HostTarget, PluginManifest};
use marqdo_sandbox::SandboxMode;
use marqdo_value::{ErrorCategory, Value};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-host-capability-{}-{id}",
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
    let output = dir.join("capability.so");
    let status = Command::new("cc")
        .args(["-std=c11", "-O2", "-shared", "-fPIC"])
        .arg("-I")
        .arg(repo_root().join("include"))
        .arg(repo_root().join("tests/abi/capability_plugin.c"))
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
  name: demo.capability
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: capability.read
      signature: "() -> Text"
  requires:
    - fs.read
  artifacts:
    - target: {}
      file: capability.so
      sha256: "{}"
"#,
        HostTarget::current().triple(),
        sha
    );
    std::fs::write(dir.join("plugin.yaml"), yaml).unwrap();
}

#[test]
fn granted_fs_read_returns_real_bytes_and_denial_is_audited() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/allowed.txt"), "hello-from-project").unwrap();
    let artifact = build_plugin(dir.path());
    write_manifest(dir.path(), &sha256_file(&artifact).unwrap());
    let manifest = PluginManifest::load(&dir.path().join("plugin.yaml")).unwrap();

    let capabilities = Capabilities::project()
        .allow_plugin("demo.capability")
        .allow_read("data/**");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    host.load_plugin(&manifest, dir.path(), true).unwrap();
    let ctx = host.context("root:default::test.mq.md", None);
    let value = host
        .invoke(
            HostOp::PluginInvoke,
            vec![
                Value::Text("capability.read".into()),
                Value::Map(Default::default()),
            ],
            &ctx,
        )
        .unwrap_or_else(|error| panic!("{error:?} audit={:?}", host.capability_audit()));
    assert_eq!(value, Value::Text("hello-from-project".into()));

    let audit = host.capability_audit();
    assert!(
        audit
            .iter()
            .any(|entry| entry.capability == "fs.read" && entry.decision == "granted"),
        "{audit:?}"
    );
    assert!(
        !format!("{audit:?}").contains("hello-from-project"),
        "audit must not record file contents"
    );

    // Same load, but no read grant: the broker must not return bytes.
    let capabilities = Capabilities::project().allow_plugin("demo.capability");
    let mut denied_host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    denied_host
        .load_plugin(&manifest, dir.path(), true)
        .unwrap();
    let ctx = denied_host.context("root:default::test.mq.md", None);
    let error = denied_host
        .invoke(
            HostOp::PluginInvoke,
            vec![
                Value::Text("capability.read".into()),
                Value::Map(Default::default()),
            ],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
    let audit = denied_host.capability_audit();
    assert!(
        audit
            .iter()
            .any(|entry| entry.capability == "fs.read" && entry.decision == "denied"),
        "{audit:?}"
    );
}
