//! ABI v1 conformance tests: compile the C example plugin, load it through
//! `libloading`, round-trip a wire request, and exercise the negative paths
//! required by blueprint section 10.7.
//!
//! These tests are `unix`-only, use `cc` to build the example, and skip with a
//! message when `cc` is unavailable.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use marqdo_plugin_abi::{
    sha256_file, ErrorKind, HostTarget, LoadedPlugin, PluginManifest, Value, WireRequest,
};

static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Minimal dependency-free temporary directory.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "marqdo-plugin-abi-test-{}-{unique}-{tag}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn have_cc() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn skip_without_cc(test_name: &str) -> bool {
    if have_cc() {
        return false;
    }
    eprintln!("skipping `{test_name}`: `cc` is not available in PATH");
    true
}

fn compile_c(dir: &Path, source: &Path, output_name: &str, extra_flags: &[&str]) -> PathBuf {
    let output = dir.join(output_name);
    let mut command = Command::new("cc");
    command
        .arg("-std=c11")
        .arg("-O2")
        .arg("-shared")
        .arg("-fPIC")
        .arg("-I")
        .arg(repo_root().join("include"))
        .args(extra_flags)
        .arg(source)
        .arg("-o")
        .arg(&output);
    let result = command.output().expect("spawn cc");
    assert!(
        result.status.success(),
        "cc failed for {}: {}",
        source.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    output
}

fn compile_example(dir: &Path, extra_flags: &[&str]) -> PathBuf {
    compile_c(
        dir,
        &repo_root().join("examples/plugin-demo/solver.c"),
        "solver.so",
        extra_flags,
    )
}

fn manifest_yaml(target: &str, sha256: &str) -> String {
    format!(
        r#"plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: {target}
      file: solver.so
      sha256: "{sha256}"
"#
    )
}

fn write_manifest(dir: &Path, target: &str, sha256: &str) -> PathBuf {
    let path = dir.join("plugin.yaml");
    std::fs::write(&path, manifest_yaml(target, sha256)).expect("write manifest");
    path
}

fn host_triple() -> String {
    HostTarget::current().triple().to_string()
}

fn open_plugin(dir: &Path) -> Result<LoadedPlugin, marqdo_plugin_abi::PluginError> {
    let manifest = PluginManifest::load(&dir.join("plugin.yaml"))?;
    LoadedPlugin::open(&manifest, dir)
}

#[test]
fn c_plugin_roundtrips_wire_request() {
    if skip_without_cc("c_plugin_roundtrips_wire_request") {
        return;
    }
    let dir = TempDir::new("roundtrip");
    let artifact = compile_example(dir.path(), &[]);
    let sha256 = sha256_file(&artifact).expect("hash");
    write_manifest(dir.path(), &host_triple(), &sha256);

    let mut plugin = open_plugin(dir.path()).expect("load plugin");
    assert_eq!(plugin.abi_version().major, 1);
    assert_eq!(plugin.abi_version().minor, 0);
    assert_eq!(plugin.metadata().name, "demo.solver");
    assert_eq!(plugin.metadata().version, "0.1.0");
    let exports: Vec<&str> = plugin.exports().map(|export| export.name()).collect();
    assert_eq!(exports, vec!["solver.add"]);

    let request = WireRequest::new("solver.add")
        .with_arg("a", &Value::Int(20_i64.into()))
        .with_arg("b", &Value::Int(22_i64.into()))
        .with_trace_id("conformance-test");
    let response = plugin.invoke(&request).expect("invoke");
    let request_json = String::from_utf8(request.to_json_bytes().expect("encode")).expect("utf8");
    assert!(request_json.contains("\"wire_version\":1"));
    assert!(request_json.contains("\"type\":\"int\""));
    assert_eq!(
        response.typed_value().expect("typed value"),
        Some(Value::Int(42_i64.into()))
    );

    // Plugin-side handler errors arrive as structured errors.
    let bad_request = WireRequest::new("solver.add").with_arg("a", &Value::Int(1_i64.into()));
    let error = plugin.invoke(&bad_request).expect_err("missing b");
    assert_eq!(error.kind, ErrorKind::Invoke);
    assert_eq!(error.code, "bad_arguments");

    // Unknown exports are rejected by the host bridge before calling the plugin.
    let unknown = WireRequest::new("solver.nope");
    assert_eq!(
        plugin.invoke(&unknown).expect_err("unknown export").code,
        "unknown_export"
    );
}

#[test]
fn wrong_abi_major_fails_load() {
    if skip_without_cc("wrong_abi_major_fails_load") {
        return;
    }
    let dir = TempDir::new("bad-abi");
    let artifact = compile_example(dir.path(), &["-DMQ_SOLVER_REPORT_ABI_MAJOR=2"]);
    let sha256 = sha256_file(&artifact).expect("hash");
    write_manifest(dir.path(), &host_triple(), &sha256);

    let error = open_plugin(dir.path()).expect_err("ABI 2 must not load");
    assert_eq!(error.kind, ErrorKind::Abi);
    assert_eq!(error.code, "abi_major_mismatch");
}

#[test]
fn missing_entry_symbol_fails_load() {
    if skip_without_cc("missing_entry_symbol_fails_load") {
        return;
    }
    let dir = TempDir::new("no-entry");
    let source = dir.path().join("not_a_plugin.c");
    std::fs::write(
        &source,
        "/* intentionally exports a different symbol */\n\
         #include <stdint.h>\n\
         int32_t mq_plugin_entry_v0(void) { return 0; }\n",
    )
    .expect("write source");
    let artifact = compile_c(dir.path(), &source, "solver.so", &[]);
    let sha256 = sha256_file(&artifact).expect("hash");
    write_manifest(dir.path(), &host_triple(), &sha256);

    let error = open_plugin(dir.path()).expect_err("missing symbol must fail");
    assert_eq!(error.kind, ErrorKind::Load);
    assert_eq!(error.code, "missing_entry_symbol");
}

#[test]
fn missing_artifact_file_fails_resolution() {
    let dir = TempDir::new("missing-artifact");
    // Declared artifact is never created.
    write_manifest(dir.path(), &host_triple(), &"0".repeat(64));

    let error = open_plugin(dir.path()).expect_err("missing artifact must fail");
    assert_eq!(error.kind, ErrorKind::Artifact);
    assert_eq!(error.code, "missing_artifact");
}

#[test]
fn sha256_mismatch_fails_verification() {
    if skip_without_cc("sha256_mismatch_fails_verification") {
        return;
    }
    let dir = TempDir::new("hash-mismatch");
    compile_example(dir.path(), &[]);
    write_manifest(dir.path(), &host_triple(), &"0".repeat(64));

    let error = open_plugin(dir.path()).expect_err("hash mismatch must fail");
    assert_eq!(error.kind, ErrorKind::Artifact);
    assert_eq!(error.code, "hash_mismatch");
}

#[test]
fn malformed_manifest_is_rejected() {
    let dir = TempDir::new("bad-manifest");
    std::fs::write(dir.path().join("plugin.yaml"), "plugin: [unclosed").expect("write");
    let error = PluginManifest::load(&dir.path().join("plugin.yaml")).expect_err("parse");
    assert_eq!(error.kind, ErrorKind::Manifest);
    assert_eq!(error.code, "manifest_parse_failed");

    // Structurally valid YAML but semantically invalid manifest.
    let invalid = manifest_yaml(&host_triple(), &"0".repeat(64))
        .replace("version: 0.1.0", "version: 01.0.0")
        .replace("name: solver.add", "name: not-namespaced");
    let manifest = PluginManifest::parse_str(&invalid).expect("parse");
    let error = manifest.validate().expect_err("invalid manifest");
    assert_eq!(error.kind, ErrorKind::Manifest);
}

#[test]
fn artifact_target_must_match_host() {
    let dir = TempDir::new("target-mismatch");
    write_manifest(
        dir.path(),
        "definitely-not-the-host-target-triple",
        &"0".repeat(64),
    );

    let error = open_plugin(dir.path()).expect_err("target mismatch must fail");
    assert_eq!(error.kind, ErrorKind::Artifact);
    // Either the target triple is rejected as invalid or it does not match.
    assert!(
        error.code == "invalid_target_triple" || error.code == "no_matching_artifact",
        "unexpected code {}",
        error.code
    );
}

#[test]
fn example_manifest_parses_and_validates() {
    let manifest = PluginManifest::load(&repo_root().join("examples/plugin-demo/plugin.yaml"))
        .expect("demo manifest parses");
    manifest.validate().expect("demo manifest validates");
    assert_eq!(manifest.plugin.name, "demo.solver");
    assert_eq!(manifest.package_version().expect("version").raw, "0.1.0");
    assert_eq!(manifest.abi_requirement().expect("abi").raw, "^1.0");
}
