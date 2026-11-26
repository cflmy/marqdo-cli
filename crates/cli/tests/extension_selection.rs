//! Extension selection and lock v4 conformance (RFC-EXT-0001 §6.6).
//!
//! These are black-box CLI tests: `marqdo.yaml.extensions` declares activation
//! sources, `marqdo lock` records their exact identity, and `check`/`run`
//! compare the current declaration with the lock before using any extension.
#![cfg(unix)]
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use marqdo_host::plugin::{sha256_file, HostTarget};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-ext-selection-{}-{id}-{tag}",
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

fn have_cc() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "plugin.dylib"
    } else {
        "plugin.so"
    }
}

fn compile_plugin(dir: &Path) -> PathBuf {
    let output = dir.join(library_name());
    let flags: &[&str] = if cfg!(target_os = "macos") {
        &["-dynamiclib", "-fPIC"]
    } else {
        &["-shared", "-fPIC"]
    };
    let status = Command::new("cc")
        .args(["-std=c11", "-O2"])
        .args(flags)
        .arg("-I")
        .arg(repo().join("include"))
        .arg(repo().join("examples/plugin-demo/solver.c"))
        .arg("-o")
        .arg(&output)
        .status()
        .expect("cc");
    assert!(status.success(), "cc failed");
    output
}

fn write_plugin_manifest(adapter: &Path, artifact: &Path) {
    let sha = sha256_file(artifact).unwrap();
    std::fs::write(
        adapter.join("plugin.yaml"),
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
      sha256: "{sha}"
"#,
            HostTarget::current().triple(),
            artifact.file_name().unwrap().to_string_lossy()
        ),
    )
    .unwrap();
}

fn write_project(project: &Path, extension_yaml: &str) {
    std::fs::create_dir_all(project.join("adapter")).unwrap();
    std::fs::write(
        project.join("marqdo.yaml"),
        format!("project:\n  name: ext-project\n  version: 0.1.0\n{extension_yaml}"),
    )
    .unwrap();
    std::fs::write(
        project.join("index.mq.md"),
        "## @exec main\n\n`return solver.add(a=20, b=22)`\n",
    )
    .unwrap();
}

/// Build a project with one worker plugin declared through `extensions:`.
fn plugin_project(tag: &str) -> Option<TempDir> {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return None;
    }
    let project = TempDir::new(tag);
    let artifact = compile_plugin(&project.path().join("adapter").create_dir_all_ok());
    write_plugin_manifest(&project.path().join("adapter"), &artifact);
    write_project(
        project.path(),
        "extensions:\n  plugins:\n    - from: ./adapter/plugin.yaml\n",
    );
    Some(project)
}

fn lock(project: &Path) -> Output {
    cli(&["lock"], project)
}

#[test]
fn declared_plugin_extension_locks_and_runs() {
    let Some(project) = plugin_project("positive") else {
        return;
    };
    let locked = lock(project.path());
    assert_eq!(locked.status.code(), Some(0), "{}", stderr(&locked));
    let lock_text = std::fs::read_to_string(project.path().join("marqdo.lock")).unwrap();
    assert!(lock_text.contains("lock_version: 4"));
    assert!(lock_text.contains("kind: plugin"));
    assert!(lock_text.contains("source: ./adapter/plugin.yaml"));
    let check = cli(&["check", "index.mq.md"], project.path());
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let run = cli(
        &["run", "index.mq.md", "--allow-plugin", "demo.solver"],
        project.path(),
    );
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
}

#[test]
fn declared_extension_without_a_lock_is_fail_closed() {
    let Some(project) = plugin_project("no-lock") else {
        return;
    };
    let check = cli(&["check", "index.mq.md"], project.path());
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("ext.extension_lock_mismatch"),
        "{}",
        stderr(&check)
    );
    assert!(stderr(&check).contains("marqdo lock"), "{}", stderr(&check));
}

#[test]
fn changed_manifest_after_lock_is_rejected() {
    let Some(project) = plugin_project("manifest-change") else {
        return;
    };
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let adapter = project.path().join("adapter");
    let manifest = std::fs::read_to_string(adapter.join("plugin.yaml")).unwrap();
    std::fs::write(
        adapter.join("plugin.yaml"),
        manifest.replace("version: 0.1.0", "version: 0.2.0"),
    )
    .unwrap();
    let check = cli(&["check", "index.mq.md"], project.path());
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("ext.extension_lock_mismatch"),
        "{}",
        stderr(&check)
    );
}

#[test]
fn changed_artifact_after_lock_fails_before_main_runs() {
    let Some(project) = plugin_project("artifact-change") else {
        return;
    };
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let artifact = project.path().join("adapter").join(library_name());
    let mut bytes = std::fs::read(&artifact).unwrap();
    bytes.push(0);
    std::fs::write(&artifact, bytes).unwrap();
    let run = cli(
        &["run", "index.mq.md", "--allow-plugin", "demo.solver"],
        project.path(),
    );
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("ext.artifact_integrity"),
        "{}",
        stderr(&run)
    );
    assert!(stdout(&run).is_empty());
}

#[test]
fn extension_source_traversal_is_rejected_at_load() {
    let project = TempDir::new("traversal");
    write_project(
        project.path(),
        "extensions:\n  plugins:\n    - from: ../escape/plugin.yaml\n",
    );
    let check = cli(&["check", "index.mq.md"], project.path());
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("extension") || stderr(&check).contains("must stay inside"),
        "{}",
        stderr(&check)
    );
}

#[test]
fn undeclared_dependency_alias_is_rejected() {
    let project = TempDir::new("alias");
    write_project(
        project.path(),
        "extensions:\n  plugins:\n    - from: \"@missing/plugin.yaml\"\n",
    );
    let check = cli(&["check", "index.mq.md"], project.path());
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("not a declared dependency"),
        "{}",
        stderr(&check)
    );
}

#[test]
fn lock_v3_is_rejected_with_an_explicit_migration() {
    let Some(project) = plugin_project("v3") else {
        return;
    };
    let lock_path = project.path().join("marqdo.lock");
    std::fs::write(
        &lock_path,
        "lock_version: 3\ncontent_profile_version: 1\nroot:\n  package: root:ext-project@0.1.0\n  manifest_digest: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\npackages: []\nroot_dependencies: {}\n",
    )
    .unwrap();
    let check = cli(&["check", "index.mq.md"], project.path());
    assert!(!check.status.success(), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("lock v3 is superseded by lock v4"),
        "{}",
        stderr(&check)
    );
    assert!(!stdout(&check).contains("ok"));
}

#[test]
fn pure_and_strict_refuse_project_declared_extensions() {
    let Some(project) = plugin_project("modes") else {
        return;
    };
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let pure = cli(&["run", "index.mq.md", "--mode", "pure"], project.path());
    assert_eq!(pure.status.code(), Some(2), "{}", stderr(&pure));
    let strict = cli(&["run", "index.mq.md", "--mode", "strict"], project.path());
    assert_eq!(strict.status.code(), Some(2), "{}", stderr(&strict));
}

#[test]
fn provider_extension_is_locked_but_still_requires_an_exact_grant() {
    let project = TempDir::new("provider");
    std::fs::create_dir_all(project.path().join("vendor/lib")).unwrap();
    let artifact = project.path().join("vendor/lib/libsensor.so");
    std::fs::write(&artifact, b"fixture provider artifact").unwrap();
    let sha = sha256_file(&artifact).unwrap();
    std::fs::write(
        project.path().join("vendor/provider.json"),
        format!(
            r#"{{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [{{"target": "{}", "path": "lib/libsensor.so", "sha256": "{sha}"}}],
  "permissions_requested": []
}}"#,
            HostTarget::current().triple()
        ),
    )
    .unwrap();
    write_project(
        project.path(),
        "extensions:\n  providers:\n    - from: ./vendor/provider.json\n",
    );
    std::fs::write(
        project.path().join("index.mq.md"),
        "## @exec main\n\n`流 = resource.open(\"demo.sensor\", \"unit-1\", [\"read\"])`\n\n`return resource.close(流)`\n",
    )
    .unwrap();
    let locked = lock(project.path());
    assert_eq!(locked.status.code(), Some(0), "{}", stderr(&locked));
    let lock_text = std::fs::read_to_string(project.path().join("marqdo.lock")).unwrap();
    assert!(lock_text.contains("kind: provider"));
    // Locked identity is not a grant: the provider is selected from the
    // manifest but `--allow-provider` is still required before any worker.
    let no_grant = cli(&["run", "index.mq.md"], project.path());
    assert_eq!(no_grant.status.code(), Some(1), "{}", stderr(&no_grant));
    assert!(
        stderr(&no_grant).contains("permission.denied"),
        "{}",
        stderr(&no_grant)
    );
}

trait CreateDirOk {
    fn create_dir_all_ok(&self) -> PathBuf;
}

impl CreateDirOk for PathBuf {
    fn create_dir_all_ok(&self) -> PathBuf {
        std::fs::create_dir_all(self).unwrap();
        self.clone()
    }
}

#[test]
fn inspect_extensions_reports_lock_identity_without_executing() {
    let Some(project) = plugin_project("inspect") else {
        return;
    };
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let artifact = project.path().join("adapter").join(library_name());
    std::fs::remove_file(&artifact).unwrap();
    // Inspection is static: a missing artifact does not prevent identity
    // reporting, and no worker is started.
    let output = cli(
        &["inspect", "index.mq.md", "--extensions", "--json"],
        project.path(),
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let declared = report["extensions"]["declared"].as_array().unwrap();
    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0]["kind"], "plugin");
    assert_eq!(declared[0]["status"], "locked");
    assert_eq!(declared[0]["authorization"], "not_granted");
    assert_eq!(declared[0]["name"], "demo.solver");
    let plugins = report["extensions"]["plugins"].as_array().unwrap();
    assert_eq!(plugins[0]["package"], "demo.solver");
    assert_eq!(plugins[0]["exports"][0]["name"], "solver.add");
    assert_eq!(plugins[0]["authorization"], "not_granted");
}

#[test]
fn cli_and_manifest_selection_of_the_same_plugin_is_idempotent() {
    let Some(project) = plugin_project("idempotent") else {
        return;
    };
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let adapter = project.path().join("adapter");
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            adapter.to_str().unwrap(),
        ],
        project.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn two_different_manifests_for_one_package_conflict() {
    let Some(project) = plugin_project("conflict") else {
        return;
    };
    let second = project.path().join("adapter-v2");
    std::fs::create_dir_all(&second).unwrap();
    let artifact = project.path().join("adapter").join(library_name());
    std::fs::copy(&artifact, second.join(library_name())).unwrap();
    write_plugin_manifest(&second, &second.join(library_name()));
    let manifest = std::fs::read_to_string(second.join("plugin.yaml")).unwrap();
    std::fs::write(
        second.join("plugin.yaml"),
        manifest.replace("version: 0.1.0", "version: 0.2.0"),
    )
    .unwrap();
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            project.path().join("adapter").to_str().unwrap(),
            "--plugin",
            second.to_str().unwrap(),
        ],
        project.path(),
    );
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("ext.duplicate_callable"),
        "{}",
        stderr(&check)
    );
}

#[test]
fn declared_kernel_extension_is_activated_from_the_manifest() {
    let project = TempDir::new("kernel");
    std::fs::create_dir_all(project.path().join("kernel")).unwrap();
    let provider = project.path().join("kernel/mocklang");
    std::fs::write(
        &provider,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"type\":\"int\",\"decimal\":\"42\"}'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let digest = sha256_file(&provider).unwrap();
    std::fs::write(
        project.path().join("kernel/kernel.json"),
        format!(
            r#"{{
  "provider_id": "mocklang-provider",
  "languages": ["mocklang-only"],
  "wire_version": 1,
  "entrypoint": "{}",
  "permissions_requested": [],
  "digest": "{digest}"
}}"#,
            provider.display()
        ),
    )
    .unwrap();
    write_project(
        project.path(),
        "extensions:\n  kernels:\n    - from: ./kernel/kernel.json\n",
    );
    std::fs::write(
        project.path().join("index.mq.md"),
        "## @exec main\n\n`结果 = foreign.call(ref(\"#代码\"), {})`\n\n`return 结果`\n\n### @knowledge 代码\n\n```mocklang-only\ninert\n```\n",
    )
    .unwrap();
    assert_eq!(lock(project.path()).status.code(), Some(0));
    let lock_text = std::fs::read_to_string(project.path().join("marqdo.lock")).unwrap();
    assert!(lock_text.contains("kind: kernel"));
    let run = cli(&["run", "index.mq.md", "--allow-foreign"], project.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
}

#[test]
fn retired_extension_descriptor_revisions_are_rejected_without_migration() {
    // A kernel descriptor is only current at wire_version 1. There is no v2
    // reader and no migration: the descriptor is rejected before locking.
    let project = TempDir::new("retired-kernel");
    std::fs::create_dir_all(project.path().join("kernel")).unwrap();
    std::fs::write(
        project.path().join("kernel/kernel.json"),
        r#"{
  "provider_id": "retired-provider",
  "languages": ["retired-lang"],
  "wire_version": 2,
  "entrypoint": "./retired-lang",
  "permissions_requested": [],
  "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}"#,
    )
    .unwrap();
    write_project(
        project.path(),
        "extensions:\n  kernels:\n    - from: ./kernel/kernel.json\n",
    );
    std::fs::write(project.path().join("index.mq.md"), "## @exec main\n\nx\n").unwrap();
    let rejected = lock(project.path());
    assert!(!rejected.status.success(), "{}", stderr(&rejected));
    assert!(
        stderr(&rejected).contains("unsupported wire_version 2"),
        "{}",
        stderr(&rejected)
    );
    assert!(
        !project.path().join("marqdo.lock").exists(),
        "a rejected retired descriptor must not produce a lock"
    );

    // A plugin manifest whose ABI requirement targets a retired major is
    // rejected by the single current manifest validator.
    let Some(project) = plugin_project("retired-abi") else {
        return;
    };
    let manifest_path = project.path().join("adapter/plugin.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        manifest.replace("abi: \"^1.0\"", "abi: \"^2.0\""),
    )
    .unwrap();
    let rejected = lock(project.path());
    assert!(!rejected.status.success(), "{}", stderr(&rejected));
    assert!(
        stderr(&rejected).contains("abi_requirement_unsupported"),
        "{}",
        stderr(&rejected)
    );
}
