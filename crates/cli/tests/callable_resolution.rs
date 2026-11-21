//! Black-box callable resolution / binding conformance (RFC-EXT-0001 §4.6).
//!
//! Every test drives the real `marqdo` binary through the public CLI. The
//! point is to prove that `check` and `run` share one static resolution and
//! that the runtime never guesses a plugin from a dotted string prefix.
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
            "marqdo-callable-test-{}-{id}-{tag}",
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
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "solver.dylib"
    } else {
        "solver.so"
    }
}

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

fn compile_plugin(dir: &Path, extra: &[&str]) -> PathBuf {
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

fn write_worker_manifest(dir: &Path, artifact: &Path, exports: &str) {
    let sha = sha256_file(artifact).unwrap();
    write(
        &dir.join("plugin.yaml"),
        &format!(
            r#"plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: worker
  exports:
{exports}  requires:
    - math.compute
  artifacts:
    - target: {}
      file: {}
      sha256: "{sha}"
"#,
            HostTarget::current().triple(),
            artifact.file_name().unwrap().to_string_lossy()
        ),
    );
}

fn add_export(name: &str, signature: &str) -> String {
    format!("    - name: {name}\n      signature: \"{signature}\"\n")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

// CALL-01
#[test]
fn import_alias_conflicting_with_builtin_namespace_is_rejected() {
    let dir = TempDir::new("call01");
    write(
        &dir.path().join("math.mq.md"),
        "## @exec eval\n\n`params(a: Int)`\n\n`return a + 100`\n",
    );
    write(
        &dir.path().join("index.mq.md"),
        "---\nimport:\n  math: \"./math.mq.md\"\n---\n\n## @exec main\n\n`return math.eval(a=1)`\n",
    );
    let check = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("ext.import_namespace_conflict"),
        "{}",
        stderr(&check)
    );
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(1));
    assert!(stdout(&run).is_empty());
    assert!(
        stderr(&run).contains("ext.import_namespace_conflict"),
        "{}",
        stderr(&run)
    );
}

// CALL-02
#[test]
fn dotted_builtin_call_binds_without_an_import() {
    let output = cli(&["check", "examples/math.mq.md"], &repo());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let run = cli(&["call", "examples/math.mq.md", "计算圆面积"], &repo());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
}

// CALL-03
#[test]
fn renamed_import_alias_binds_the_module_function() {
    let dir = TempDir::new("call03");
    write(
        &dir.path().join("math.mq.md"),
        "## @exec eval\n\n`params(a: Int)`\n\n`return a + 100`\n",
    );
    write(
        &dir.path().join("index.mq.md"),
        "---\nimport:\n  mymath: \"./math.mq.md\"\n---\n\n## @exec main\n\n`return mymath.eval(a=1)`\n",
    );
    let check = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let run = cli(&["run", "index.mq.md", "--json"], dir.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(
        json["value"],
        serde_json::json!({"type": "int", "decimal": "101"})
    );
}

// CALL-04 / CALL-05
#[test]
fn unknown_plugin_export_is_a_static_symbol_error() {
    let dir = TempDir::new("call04");
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`return solver.add(a=1, b=2)`\n",
    );
    let check = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(check.status.code(), Some(1));
    assert!(
        stderr(&check).contains("symbol.unknown"),
        "{}",
        stderr(&check)
    );
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(1));
    assert!(stderr(&run).contains("symbol.unknown"), "{}", stderr(&run));
    assert!(stdout(&run).is_empty());
    // CALL-05: the diagnostic does not change when an unrelated plugin is
    // loaded, because the static export table is still the only source.
    if !have_cc() {
        return;
    }
    let plugin_dir = TempDir::new("call05-plugin");
    let artifact = compile_plugin(plugin_dir.path(), &[]);
    write_worker_manifest(
        plugin_dir.path(),
        &artifact,
        &add_export("solver.add", "($a: Int, $b: Int) -> Int"),
    );
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`return solver.subtract(a=1, b=2)`\n",
    );
    let with_plugin = cli(
        &[
            "run",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
        ],
        dir.path(),
    );
    assert_eq!(with_plugin.status.code(), Some(1));
    assert!(
        stderr(&with_plugin).contains("symbol.unknown"),
        "{}",
        stderr(&with_plugin)
    );
}

// CALL-06
#[test]
fn plugin_with_two_prefixes_binds_both_exports() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("call06");
    let plugin_dir = TempDir::new("call06-plugin");
    let artifact = compile_plugin(plugin_dir.path(), &["-DMQ_SOLVER_MULTI_EXPORT"]);
    write_worker_manifest(
        plugin_dir.path(),
        &artifact,
        &(add_export("solver.add", "($a: Int, $b: Int) -> Int")
            + &add_export("solver.neg", "($a: Int) -> Int")),
    );
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`和 = solver.add(20, b=22)`\n\n`相反数 = solver.neg(7)`\n\n`打印(和)`\n\n`打印(相反数)`\n",
    );
    let output = cli(
        &[
            "run",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
        ],
        dir.path(),
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "42\n-7\n");
}

// CALL-07
#[test]
fn duplicate_complete_export_across_plugins_is_rejected_at_prepare() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("call07");
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`return solver.add(a=1, b=2)`\n",
    );
    let mut plugins = Vec::new();
    for tag in ["a", "b"] {
        let plugin_dir = TempDir::new(tag);
        let artifact = compile_plugin(plugin_dir.path(), &[]);
        let sha = sha256_file(&artifact).unwrap();
        write(
            &plugin_dir.path().join("plugin.yaml"),
            &format!(
                r#"plugin:
  name: demo.{tag}.solver
  version: 0.1.0
  abi: "^1.0"
  kind: worker
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  artifacts:
    - target: {}
      file: {}
      sha256: "{sha}"
"#,
                HostTarget::current().triple(),
                artifact.file_name().unwrap().to_string_lossy()
            ),
        );
        plugins.push(plugin_dir);
    }
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            plugins[0].path().to_str().unwrap(),
            "--plugin",
            plugins[1].path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
    assert!(
        stderr(&check).contains("ext.duplicate_callable"),
        "{}",
        stderr(&check)
    );
}

// CALL-08
#[test]
fn bad_plugin_arguments_fail_in_check_and_run_identically() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("call08");
    let plugin_dir = TempDir::new("call08-plugin");
    let artifact = compile_plugin(plugin_dir.path(), &[]);
    write_worker_manifest(
        plugin_dir.path(),
        &artifact,
        &add_export("solver.add", "($a: Int, $b: Int) -> Int"),
    );
    for (tag, call) in [
        ("missing", "solver.add(a=1)"),
        ("unknown", "solver.add(a=1, b=2, c=3)"),
        ("duplicate", "solver.add(a=1, a=2, b=3)"),
        ("too-many-positional", "solver.add(1, 2, 3)"),
    ] {
        write(
            &dir.path().join("index.mq.md"),
            &format!("## @exec main\n\n`return {call}`\n"),
        );
        let check = cli(
            &[
                "check",
                "index.mq.md",
                "--plugin",
                plugin_dir.path().to_str().unwrap(),
            ],
            dir.path(),
        );
        assert_eq!(check.status.code(), Some(1), "{tag}: {}", stderr(&check));
        let expected = if tag == "duplicate" {
            // The expression grammar rejects duplicate named arguments before
            // call binding; either way the failure is static.
            "sem.syntax"
        } else {
            "call.arity"
        };
        assert!(
            stderr(&check).contains(expected),
            "{tag}: {}",
            stderr(&check)
        );
        let run = cli(
            &[
                "run",
                "index.mq.md",
                "--plugin",
                plugin_dir.path().to_str().unwrap(),
                "--allow-plugin",
                "demo.solver",
            ],
            dir.path(),
        );
        assert_eq!(run.status.code(), Some(1), "{tag}: {}", stderr(&run));
        assert!(stdout(&run).is_empty(), "{tag}");
        // `check` appends a summary line and `run` a refusal line, but the
        // structured call.arity diagnostic itself must be identical.
        let arity = |text: &str| {
            text.lines()
                .filter(|line| line.contains("call.arity") || line.contains("sem.syntax"))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        };
        assert!(!arity(&stderr(&check)).is_empty(), "{tag}");
        assert_eq!(
            arity(&stderr(&check)),
            arity(&stderr(&run)),
            "{tag}: check != run"
        );
    }
}

// CALL-09
#[test]
fn changed_artifact_digest_is_rejected_before_main_runs() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("call09");
    let plugin_dir = TempDir::new("call09-plugin");
    let artifact = compile_plugin(plugin_dir.path(), &[]);
    write_worker_manifest(
        plugin_dir.path(),
        &artifact,
        &add_export("solver.add", "($a: Int, $b: Int) -> Int"),
    );
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`打印(\"must-not-run\")`\n\n`return solver.add(a=1, b=2)`\n",
    );
    // Static check succeeds: it never opens the artifact.
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    // Corrupt the artifact bytes; run must reject before the entry function.
    let mut bytes = std::fs::read(&artifact).unwrap();
    bytes.push(0);
    std::fs::write(&artifact, bytes).unwrap();
    let run = cli(
        &[
            "run",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
        ],
        dir.path(),
    );
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("ext.artifact_integrity"),
        "{}",
        stderr(&run)
    );
    assert!(stdout(&run).is_empty(), "{}", stdout(&run));
}

// CALL-11
#[test]
fn local_function_reference_is_a_dynamic_language_call() {
    let dir = TempDir::new("call11");
    write(
        &dir.path().join("index.mq.md"),
        "## @exec 平方\n\n`params(n: Int)`\n\n`return n * n`\n\n## @exec 主\n\n`f = ref(\"#平方\")`\n\n`return f(n=4)`\n",
    );
    let run = cli(&["call", "index.mq.md", "主", "--json"], dir.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(
        json["value"],
        serde_json::json!({"type": "int", "decimal": "16"})
    );
}

// CALL-12
#[test]
fn calling_a_text_value_is_not_callable_and_never_reaches_the_host() {
    let dir = TempDir::new("call12");
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`f = \"text\"`\n\n`return f()`\n",
    );
    let check = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let run = cli(&["run", "index.mq.md"], dir.path());
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("call.not_callable") || stderr(&run).contains("not_callable"),
        "{}",
        stderr(&run)
    );
}

// CALL-13
#[test]
fn explicit_import_aliases_bind_same_named_functions_independently() {
    let dir = TempDir::new("call13");
    write(
        &dir.path().join("a.mq.md"),
        "## @exec value\n\n`params()`\n\n`return 10`\n",
    );
    write(
        &dir.path().join("b.mq.md"),
        "## @exec value\n\n`params()`\n\n`return 20`\n",
    );
    write(
        &dir.path().join("index.mq.md"),
        "---\nimport:\n  aa: \"./a.mq.md\"\n  bb: \"./b.mq.md\"\n---\n\n## @exec main\n\n`return aa.value() + bb.value()`\n",
    );
    let run = cli(&["run", "index.mq.md", "--json"], dir.path());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let json: serde_json::Value = serde_json::from_str(&stdout(&run)).unwrap();
    assert_eq!(
        json["value"],
        serde_json::json!({"type": "int", "decimal": "30"})
    );
}

// CALL-14
#[test]
fn check_never_executes_a_plugin_artifact_or_side_effect_script() {
    let dir = TempDir::new("call14");
    let plugin_dir = TempDir::new("call14-plugin");
    let marker = plugin_dir.path().join("executed.marker");
    let artifact = plugin_dir.path().join(library_name());
    write(
        &artifact,
        &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
    );
    let sha = sha256_file(&artifact).unwrap();
    write(
        &plugin_dir.path().join("plugin.yaml"),
        &format!(
            r#"plugin:
  name: demo.script
  version: 0.1.0
  abi: "^1.0"
  kind: worker
  exports:
    - name: demo.run
      signature: "() -> Int"
  artifacts:
    - target: {}
      file: {}
      sha256: "{sha}"
"#,
            HostTarget::current().triple(),
            artifact.file_name().unwrap().to_string_lossy()
        ),
    );
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`return demo.run()`\n",
    );
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    assert!(!marker.exists(), "check executed the artifact");
}

// CALL-15
#[test]
fn check_emit_and_run_share_the_same_static_binding_source() {
    if !have_cc() {
        eprintln!("skipping: cc is unavailable");
        return;
    }
    let dir = TempDir::new("call15");
    let plugin_dir = TempDir::new("call15-plugin");
    let artifact = compile_plugin(plugin_dir.path(), &[]);
    write_worker_manifest(
        plugin_dir.path(),
        &artifact,
        &add_export("solver.add", "($a: Int, $b: Int) -> Int"),
    );
    write(
        &dir.path().join("index.mq.md"),
        "## @exec main\n\n`return solver.add(a=1, b=2)`\n",
    );
    let analysis_path = dir.path().join("analysis.json");
    let check = cli(
        &[
            "check",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
            "--emit",
            analysis_path.to_str().unwrap(),
        ],
        dir.path(),
    );
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let analysis: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&analysis_path).unwrap()).unwrap();
    let functions = analysis["modules"]
        .as_array()
        .and_then(|modules| modules.first())
        .and_then(|module| module["functions"].as_array())
        .unwrap_or_else(|| panic!("analysis has no module functions: {analysis}"));
    assert!(!functions.is_empty());
    let run = cli(
        &[
            "run",
            "index.mq.md",
            "--plugin",
            plugin_dir.path().to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // Without the descriptor the same call is statically unknown: run/check
    // consume the same prepared binding, not a runtime name probe.
    let without = cli(&["check", "index.mq.md"], dir.path());
    assert_eq!(without.status.code(), Some(1));
    assert!(stderr(&without).contains("symbol.unknown"));
}

// CALL-16
#[test]
fn print_alias_keeps_the_existing_output_bytes() {
    let run = cli(&["run", "examples/hello.mq.md"], &repo());
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run), "你好，Marqdo\n");
}
