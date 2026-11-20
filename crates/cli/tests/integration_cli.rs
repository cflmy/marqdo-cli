//! CLI integration tests.
//!
//! These tests drive the real `marqdo` binary (`CARGO_BIN_EXE_marqdo`) with
//! [`std::process::Command`] in throw-away temporary directories. They cover
//! the public contract from `spec`/blueprint section 13: `init`, `check`,
//! `run`, `call`, `inspect`, `fmt`, multi-module imports and cycles, lock/sync
//! reproducibility, plugin inspection and plugin invocation.
//!
//! Temporary directories live under the OS temp directory and are removed on
//! drop; the repository itself is never modified.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::Digest;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/cli has a parent")
        .parent()
        .expect("crates has a parent")
        .to_path_buf()
}

/// Remove a directory tree on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "marqdo-cli-integration-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("cannot create {}: {error}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("cannot create {}: {error}", parent.display()));
        }
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative))
            .unwrap_or_else(|error| panic!("cannot read `{relative}`: {error}"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Run the CLI in `dir`.
fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("cannot run marqdo {args:?}: {error}"))
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("marqdo terminated by a signal: {:?}", output.status))
}

fn assert_exit(output: &Output, expected: i32, context: &str) {
    assert_eq!(
        exit_code(output),
        expected,
        "{context}\n  stdout: {}\n  stderr: {}",
        stdout_text(output),
        stderr_text(output)
    );
}

fn json_stdout(output: &Output) -> serde_json::Value {
    let text = stdout_text(output);
    serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!("stdout is not a single JSON document: {error}\nstdout: {text}")
    })
}

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn init_creates_a_runnable_project() {
    let dir = TempDir::new("init");
    let output = cli(dir.path(), &["init", "proj"]);
    assert_exit(&output, 0, "init must succeed");
    assert!(stdout_text(&output).contains("Initialized Marqdo project"));
    let project = dir.path().join("proj");
    assert!(project.join("marqdo.yaml").is_file(), "manifest missing");
    assert!(
        project.join("index.mq.md").is_file(),
        "entry document missing"
    );

    let check = cli(&project, &["check", "index.mq.md"]);
    assert_exit(&check, 0, "generated project must check cleanly");
    assert!(stdout_text(&check).contains("checked 1 module(s): ok"));
    assert!(stderr_text(&check).is_empty());

    let run = cli(&project, &["run", "index.mq.md"]);
    assert_exit(&run, 0, "generated project must run");
    assert_eq!(stdout_text(&run), "你好，Marqdo\n");

    let json = cli(&project, &["check", "index.mq.md", "--json"]);
    assert_exit(&json, 0, "check --json must succeed");
    let value = json_stdout(&json);
    assert_eq!(value["command"], "check");
    assert_eq!(value["success"], true);
    assert_eq!(value["errors"], 0);
    assert!(value["diagnostics"].as_array().is_some());
    assert!(
        stderr_text(&json).is_empty(),
        "--json must not mix stderr text"
    );

    // `init` refuses to overwrite an existing project.
    let again = cli(dir.path(), &["init", "proj"]);
    assert_exit(&again, 1, "init must refuse an existing manifest");
    assert!(stderr_text(&again).contains("already contains"));
}

#[test]
fn check_exit_codes_and_diagnostics() {
    let dir = TempDir::new("check");
    dir.write("good.mq.md", "## main\n\n*1 + 1*\n");
    dir.write("bad.mq.md", "## f\n\n**a *b* c**\n");
    dir.write("warning.mq.md", "**x = 1**\n\n## main\n\n*1*\n");

    let good = cli(dir.path(), &["check", "good.mq.md"]);
    assert_exit(&good, 0, "clean document");
    assert!(stdout_text(&good).contains("checked 1 module(s): ok"));

    let bad = cli(dir.path(), &["check", "bad.mq.md"]);
    assert_exit(&bad, 1, "invalid executable shape is a hard error");
    let bad_stderr = stderr_text(&bad);
    assert!(bad_stderr.contains("error[markdown.invalid_construct]"));
    assert!(bad_stderr.contains("check failed"));
    assert!(stdout_text(&bad).is_empty());

    let bad_json = cli(dir.path(), &["check", "bad.mq.md", "--json"]);
    assert_exit(&bad_json, 1, "check --json keeps the exit code");
    let value = json_stdout(&bad_json);
    assert_eq!(value["command"], "check");
    assert_eq!(value["success"], false);
    assert_eq!(value["errors"], 1);
    let diagnostics = value["diagnostics"].as_array().expect("diagnostics array");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "markdown.invalid_construct");
    assert_eq!(diagnostics[0]["severity"], "error");
    assert_eq!(diagnostics[0]["line"], 3);
    assert!(stderr_text(&bad_json).is_empty());

    let warning = cli(dir.path(), &["check", "warning.mq.md"]);
    assert_exit(&warning, 0, "a warning does not fail check");
    assert!(stdout_text(&warning).contains("checked 1 module(s): ok"));
    assert!(stderr_text(&warning).contains("warning[markdown.inert_executable]"));
    let warning_json = cli(dir.path(), &["check", "warning.mq.md", "--json"]);
    let value = json_stdout(&warning_json);
    assert_eq!(value["success"], true);
    assert_eq!(value["diagnostics"][0]["severity"], "warning");

    let missing = cli(dir.path(), &["check", "missing.mq.md"]);
    assert_exit(&missing, 2, "missing file is a usage/IO error");
    assert!(stderr_text(&missing).contains("error"));
}

#[test]
fn run_json_embeds_program_stdout() {
    let dir = TempDir::new("run-json");
    dir.write(
        "print.mq.md",
        "## main\n\n> 打印(\"你好\")\n\n> 打印(42)\n\n*None*\n",
    );
    let output = cli(dir.path(), &["run", "print.mq.md", "--json"]);
    assert_exit(&output, 0, "run --json must succeed");
    // Exactly one JSON document on stdout: program output is embedded, never
    // interleaved with the machine-readable envelope.
    let value = json_stdout(&output);
    assert_eq!(value["command"], "run");
    assert_eq!(value["success"], true);
    assert_eq!(value["stdout"], "你好\n42\n");
    assert_eq!(value["value"]["type"], "none");
    assert!(value["trace"].as_array().is_some());
    assert!(stderr_text(&output).is_empty());

    // Text mode writes program output only to stdout, no JSON envelope.
    let text = cli(dir.path(), &["run", "print.mq.md"]);
    assert_exit(&text, 0, "run must succeed");
    assert_eq!(stdout_text(&text), "你好\n42\n");
}

#[test]
fn run_refuses_documents_with_compile_errors() {
    let dir = TempDir::new("run-errors");
    dir.write("bad.mq.md", "## f\n\n**a *b* c**\n");
    let output = cli(dir.path(), &["run", "bad.mq.md"]);
    assert_exit(&output, 1, "compile errors must prevent execution");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("markdown.invalid_construct"));
    assert!(stderr.contains("refusing to run"));
    assert!(stdout_text(&output).is_empty());
}

#[test]
fn call_executes_examples_with_typed_arguments() {
    let root = repo_root();
    let examples = root.join("examples");
    let factorial = examples.join("factorial.mq.md");

    let output = cli(
        &root,
        &[
            "call",
            factorial.to_str().unwrap(),
            "数学.阶乘",
            "--arg",
            "n=5",
            "--json",
        ],
    );
    assert_exit(&output, 0, "call must execute factorial");
    let value = json_stdout(&output);
    assert_eq!(value["success"], true);
    assert_eq!(value["value"]["type"], "int");
    assert_eq!(value["value"]["decimal"], "120");

    let math = examples.join("math.mq.md");
    let output = cli(
        &root,
        &["call", math.to_str().unwrap(), "计算圆面积", "--json"],
    );
    assert_exit(&output, 0, "call must execute the math example");
    let value = json_stdout(&output);
    assert_eq!(value["value"]["type"], "float");
    let area = value["value"]["value"].as_f64().expect("float value");
    assert!(
        (area - std::f64::consts::PI * 16.0).abs() < 1e-12,
        "area={area}"
    );

    let references = examples.join("references.mq.md");
    let output = cli(
        &root,
        &["call", references.to_str().unwrap(), "计算作用力", "--json"],
    );
    assert_exit(&output, 0, "call must execute the references example");
    let value = json_stdout(&output);
    assert_eq!(value["value"]["decimal"], "6");

    let hello = examples.join("hello.mq.md");
    let output = cli(&root, &["run", hello.to_str().unwrap()]);
    assert_exit(&output, 0, "hello example");
    assert_eq!(stdout_text(&output), "你好，Marqdo\n");
}

#[test]
fn examples_run_end_to_end_including_foreign_when_available() {
    let root = repo_root();
    let multi = root
        .join("examples")
        .join("multi-module")
        .join("index.mq.md");
    let output = cli(&root, &["run", multi.to_str().unwrap()]);
    assert_exit(&output, 0, "multi-module example");
    let printed = stdout_text(&output);
    let area: f64 = printed.trim().parse().expect("printed float");
    assert!((area - std::f64::consts::PI * 4.0).abs() < 1e-12);

    if !python_available() {
        println!("skipping python foreign example: python3 is unavailable");
        return;
    }
    let foreign = root.join("examples").join("python-foreign.mq.md");
    let output = cli(
        &root,
        &["run", foreign.to_str().unwrap(), "--allow-foreign"],
    );
    assert_exit(&output, 0, "python foreign example");
    let printed = stdout_text(&output);
    assert!(
        printed.starts_with("28.27"),
        "unexpected foreign output: {printed:?}"
    );
}

#[test]
fn strict_mode_refuses_without_isolation() {
    let root = repo_root();
    let hello = root.join("examples").join("hello.mq.md");
    let output = cli(&root, &["run", hello.to_str().unwrap(), "--mode", "strict"]);
    assert_exit(
        &output,
        1,
        "strict mode must refuse when isolation is absent",
    );
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("refusing to run in strict mode"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("isolation"), "stderr: {stderr}");
    assert!(stdout_text(&output).is_empty());

    let json = cli(
        &root,
        &["run", hello.to_str().unwrap(), "--mode", "strict", "--json"],
    );
    assert_exit(&json, 1, "strict refusal keeps exit code 1 in JSON mode");
    let value = json_stdout(&json);
    assert_eq!(value["success"], false);
    assert!(value["error"]
        .as_str()
        .expect("error string")
        .contains("strict mode"));
}

#[test]
fn inspect_outputs_ast_units_signatures_ir_and_refs() {
    let root = repo_root();
    let factorial = root.join("examples").join("factorial.mq.md");
    let factorial = factorial.to_str().unwrap();

    let ast = cli(&root, &["inspect", factorial, "--ast", "--json"]);
    assert_exit(&ast, 0, "inspect --ast");
    let value = json_stdout(&ast);
    assert_eq!(value["command"], "inspect");
    assert_eq!(value["ast"]["kind"], "document");

    let units = cli(&root, &["inspect", factorial, "--units", "--json"]);
    assert_exit(&units, 0, "inspect --units");
    let value = json_stdout(&units);
    let units = value["units"].as_array().expect("units array");
    assert!(units.iter().any(|unit| unit["kind"] == "function"));

    let signatures = cli(&root, &["inspect", factorial, "--signature", "--json"]);
    assert_exit(&signatures, 0, "inspect --signature");
    let value = json_stdout(&signatures);
    let signature = value["signatures"]
        .as_array()
        .expect("signatures array")
        .iter()
        .find(|entry| entry["name"] == "数学.阶乘")
        .expect("factorial signature");
    assert_eq!(signature["signature"], "inferred");
    assert_eq!(signature["params"][0]["name"], "n");

    let ir = cli(&root, &["inspect", factorial, "--ir", "--json"]);
    assert_exit(&ir, 0, "inspect --ir");
    let value = json_stdout(&ir);
    let functions = value["ir"].as_array().expect("IR array");
    assert!(!functions.is_empty());
    assert!(functions[0]["body"].is_array());

    // A link that resolves cleanly.
    let references = root.join("examples").join("references.mq.md");
    let refs = cli(
        &root,
        &["inspect", references.to_str().unwrap(), "--refs", "--json"],
    );
    assert_exit(&refs, 0, "inspect --refs");
    let value = json_stdout(&refs);
    let edges = value["refs"].as_array().expect("refs array");
    assert!(edges.iter().any(|edge| edge["resolved"] == true));

    // A broken #anchor is reported as unresolved by `inspect --refs`.
    let dir = TempDir::new("refs");
    dir.write(
        "broken.mq.md",
        "## main\n\n见[断链](#不存在的标题)。\n\n*1*\n",
    );
    let broken = cli(dir.path(), &["inspect", "broken.mq.md", "--refs", "--json"]);
    assert_exit(&broken, 0, "inspect --refs never executes the document");
    let value = json_stdout(&broken);
    let edges = value["refs"].as_array().expect("refs array");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["url"], "#不存在的标题");
    assert_eq!(edges[0]["resolved"], false);
}

#[test]
fn fmt_normalises_line_endings_and_trailing_newlines() {
    let dir = TempDir::new("fmt");
    dir.write("doc.mq.md", "## main\r\n\r\n*1*\r\n\r\n\r\n");
    let before = cli(dir.path(), &["fmt", "--check", "doc.mq.md"]);
    assert_exit(&before, 1, "unformatted document is detected");
    assert!(stderr_text(&before).contains("is not formatted"));

    let format = cli(dir.path(), &["fmt", "doc.mq.md"]);
    assert_exit(&format, 0, "fmt must rewrite the document");
    let formatted = dir.read("doc.mq.md");
    assert_eq!(formatted, "## main\n\n*1*\n");
    assert!(!formatted.contains('\r'));

    let after = cli(dir.path(), &["fmt", "--check", "doc.mq.md"]);
    assert_exit(&after, 0, "formatted document is accepted");
}

#[test]
fn multi_module_imports_and_cycles() {
    let dir = TempDir::new("modules");
    dir.write(
        "geometry.mq.md",
        "## 圆面积\n\n`(r: Number)`\n\n*math.eval(ref(\"#公式\"), {\"r\": r}, target=\"A\")*\n\n### 公式\n\n$$\nA = \\pi r^2\n$$\n",
    );
    dir.write(
        "index.mq.md",
        "---\nimport:\n  geometry: \"./geometry.mq.md\"\n---\n\n## main\n\n**面积 = geometry.圆面积(r=2)**\n\n> 打印(面积)\n",
    );
    let run = cli(dir.path(), &["run", "index.mq.md"]);
    assert_exit(&run, 0, "multi-module import must run");
    let printed: f64 = stdout_text(&run).trim().parse().expect("printed float");
    assert!((printed - std::f64::consts::PI * 4.0).abs() < 1e-12);
    let check = cli(dir.path(), &["check", "index.mq.md"]);
    assert_exit(&check, 0, "multi-module import must check");

    // Import cycle diagnostics.
    let cycle = TempDir::new("cycle");
    cycle.write(
        "a.mq.md",
        "---\nimport:\n  b: \"./b.mq.md\"\n---\n\n## main\n\n*1*\n",
    );
    cycle.write(
        "b.mq.md",
        "---\nimport:\n  a: \"./a.mq.md\"\n---\n\n## f\n\n*1*\n",
    );
    let output = cli(cycle.path(), &["check", "a.mq.md"]);
    assert_exit(&output, 1, "import cycle is an error");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("module.cycle"), "stderr: {stderr}");
    assert!(stderr.contains("import cycle"), "stderr: {stderr}");

    // A missing import target is a module.unresolved error.
    let missing = TempDir::new("missing-import");
    missing.write(
        "index.mq.md",
        "---\nimport:\n  gone: \"./gone.mq.md\"\n---\n\n## main\n\n*1*\n",
    );
    let output = cli(missing.path(), &["check", "index.mq.md"]);
    assert_exit(&output, 1, "missing import target is an error");
    assert!(stderr_text(&output).contains("module.unresolved"));
}

#[test]
fn lock_and_sync_locked_detect_dependency_tampering() {
    let dir = TempDir::new("lock");
    dir.write(
        "marqdo.yaml",
        "project:\n  name: locktest\n  version: 0.1.0\ndependencies:\n  dep:\n    path: \"./dep\"\n",
    );
    dir.write("index.mq.md", "## main\n\n*1*\n");
    dir.write("dep/index.mq.md", "## main\n\n*1*\n");

    let lock = cli(dir.path(), &["lock"]);
    assert_exit(&lock, 0, "lock must succeed");
    assert!(stdout_text(&lock).contains("locked 1 package(s)"));
    let lockfile = dir.read("marqdo.lock");
    assert!(lockfile.contains("name: dep"));

    let sync = cli(dir.path(), &["sync", "--locked"]);
    assert_exit(&sync, 0, "sync --locked must succeed after lock");
    assert!(stdout_text(&sync).contains("synchronized 1 package(s)"));

    // Tamper with the dependency contents: `sync --locked` must fail.
    dir.write("dep/index.mq.md", "## main\n\n*2*\n");
    let tampered = cli(dir.path(), &["sync", "--locked"]);
    assert_exit(&tampered, 1, "tampered dependency must fail sync --locked");
    assert!(stderr_text(&tampered).contains("lockfile is out of date"));

    // Re-locking restores a consistent state.
    let relock = cli(dir.path(), &["lock"]);
    assert_exit(&relock, 0, "re-lock after tampering");
    let synced = cli(dir.path(), &["sync", "--locked"]);
    assert_exit(&synced, 0, "sync --locked after re-lock");
}

/// Host target and dynamic-library naming for a real Native ABI fixture.
///
/// The manifest is generated from the actual host target instead of reusing
/// the Linux-only example manifest. This is what makes the integration test a
/// real test on macOS/Windows rather than an accidental "no matching artifact".
fn host_triple() -> String {
    marqdo_plugin_abi::artifact::current_target_triple()
        .as_str()
        .to_string()
}

fn dynamic_library_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "solver.dll"
    } else if cfg!(target_os = "macos") {
        "solver.dylib"
    } else {
        "solver.so"
    }
}

fn shared_library_flags() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["-shared"]
    } else if cfg!(target_os = "macos") {
        &["-dynamiclib", "-fPIC"]
    } else {
        &["-shared", "-fPIC"]
    }
}

/// Build the C demo plugin in a temporary directory and generate a manifest for
/// this host. The repository (including `examples/plugin-demo/plugin.yaml`) is
/// never modified, and no prebuilt artifact is reused.
fn build_plugin_copy(tag: &str) -> Option<(TempDir, PathBuf)> {
    build_plugin_copy_with(tag, &[])
}

/// Like [`build_plugin_copy`], with extra C compiler flags for negative fixtures.
fn build_plugin_copy_with(tag: &str, extra_flags: &[&str]) -> Option<(TempDir, PathBuf)> {
    if !cc_available() {
        // A real C ABI test needs a C toolchain. Windows MSVC does not provide
        // `cc` in the plain runner PATH, so native ABI on Windows is reported
        // as unsupported rather than counted as a pass.
        if cfg!(windows) {
            println!("skipping native plugin test on Windows: no `cc` toolchain");
            return None;
        }
        panic!("`cc` is required for the native plugin integration test on this platform");
    }
    let dir = TempDir::new(tag);
    let target = dir.path().join("plugin-demo");
    fs::create_dir_all(&target).expect("create plugin copy");
    let source = repo_root().join("examples").join("plugin-demo");
    fs::copy(source.join("solver.c"), target.join("solver.c")).expect("copy plugin source");

    let artifact = target.join(dynamic_library_name());
    let status = Command::new("cc")
        .args(shared_library_flags())
        .args(extra_flags)
        .arg("-I")
        .arg(repo_root().join("include"))
        .args(["-o", artifact.to_str().expect("utf-8 path")])
        .arg(target.join("solver.c"))
        .status()
        .unwrap_or_else(|error| panic!("cannot run cc: {error}"));
    assert!(status.success(), "building the demo plugin failed");

    let bytes = fs::read(&artifact).expect("read built plugin");
    let digest = hex::encode(sha2::Sha256::digest(&bytes));
    let manifest = format!(
        "plugin:\n  name: demo.solver\n  version: 0.1.0\n  abi: \"^1.0\"\n  kind: native\n  exports:\n    - name: solver.add\n      signature: \"($a: Int, $b: Int) -> Int\"\n  requires:\n    - math.compute\n  artifacts:\n    - target: {}\n      file: {}\n      sha256: \"{}\"\n",
        host_triple(),
        dynamic_library_name(),
        digest
    );
    fs::write(target.join("plugin.yaml"), manifest).expect("write plugin.yaml");
    Some((dir, target))
}

#[test]
fn plugin_inspect_and_explicit_trusted_invocation() {
    let Some((_dir, plugin_dir)) = build_plugin_copy("plugin") else {
        return;
    };
    // Inspection never loads plugin code and verifies the artifact hash.
    let inspect = cli(
        repo_root().as_path(),
        &["plugin", "inspect", plugin_dir.to_str().unwrap(), "--json"],
    );
    assert_exit(&inspect, 0, "plugin inspect");
    let value = json_stdout(&inspect);
    assert_eq!(value["command"], "plugin.inspect");
    assert_eq!(value["success"], true);
    assert_eq!(value["loaded"], false);
    assert_eq!(value["artifact"]["verified"], true);
    assert_eq!(value["exports"][0]["name"], "solver.add");

    // Invoking a plugin export from a document requires an explicit trust
    // decision, a permission grant and the plugin load itself.
    let dir = TempDir::new("plugin-run");
    dir.write(
        "doc.mq.md",
        "## main\n\n**结果 = solver.add(a=20, b=22)**\n\n> 打印(结果)\n",
    );
    let plugin_path = plugin_dir.to_str().unwrap();
    let doc = dir.path().join("doc.mq.md");
    let doc = doc.to_str().unwrap();

    let output = cli(
        dir.path(),
        &[
            "run",
            doc,
            "--plugin",
            plugin_path,
            "--trust-native-plugin",
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_exit(&output, 0, "trusted plugin invocation");
    assert_eq!(stdout_text(&output), "42\n");

    let untrusted = cli(
        dir.path(),
        &[
            "run",
            doc,
            "--plugin",
            plugin_path,
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_exit(&untrusted, 1, "in-process loading needs explicit trust");
    assert!(stderr_text(&untrusted).contains("--trust-native-plugin"));

    let ungranted = cli(
        dir.path(),
        &["run", doc, "--plugin", plugin_path, "--trust-native-plugin"],
    );
    assert_exit(&ungranted, 1, "plugin permission is required");
    assert!(stderr_text(&ungranted).contains("permission.denied"));

    // Unknown plugin namespace is a hard runtime symbol error.
    dir.write(
        "unknown.mq.md",
        "## main\n\n**结果 = other.add(a=1, b=1)**\n",
    );
    let unknown = cli(
        dir.path(),
        &[
            "run",
            "unknown.mq.md",
            "--plugin",
            plugin_path,
            "--trust-native-plugin",
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_exit(&unknown, 1, "unknown plugin namespace");
    assert!(stderr_text(&unknown).contains("symbol.unknown"));

    // The export signature requires both named arguments: with a declared
    // plugin manifest this is rejected statically, before any library load.
    dir.write("args.mq.md", "## main\n\n**结果 = solver.add(a=1)**\n");
    let bad_args = cli(
        dir.path(),
        &[
            "run",
            "args.mq.md",
            "--plugin",
            plugin_path,
            "--trust-native-plugin",
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_exit(&bad_args, 1, "plugin argument failure");
    assert!(stderr_text(&bad_args).contains("call.arity"));
    assert!(stderr_text(&bad_args).contains("missing required argument `b`"));
}

#[test]
fn runtime_error_in_imported_module_reports_that_module() {
    let dir = TempDir::new("cross-module-error");
    dir.write(
        "index.mq.md",
        "---\nimport:\n  physics: \"./physics.mq.md\"\n---\n\n## main\n\n> 打印(physics.boom())\n",
    );
    dir.write(
        "physics.mq.md",
        "## boom\n\n说明文字。\n\n**x = 1 / 0**\n\n*x*\n",
    );

    let output = cli(dir.path(), &["run", "index.mq.md", "--json"]);
    assert_exit(&output, 1, "division by zero is a runtime error");
    let value: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("run --json emits JSON");
    let error = &value["error"];
    assert_eq!(error["code"], "runtime.division_by_zero");
    assert!(
        error["module_id"]
            .as_str()
            .unwrap_or_default()
            .ends_with("::physics.mq.md"),
        "{}",
        error["module_id"]
    );
    assert_eq!(error["relative_path"], "physics.mq.md");
    assert!(
        error["file"]
            .as_str()
            .unwrap_or_default()
            .ends_with("physics.mq.md"),
        "{}",
        error["file"]
    );
    let line = error["line"].as_u64().unwrap_or(0);
    assert!(line >= 5, "expected the failing statement line, got {line}");
    assert!(
        !error["file"]
            .as_str()
            .unwrap_or_default()
            .ends_with("index.mq.md"),
        "must not blame the entry file"
    );

    let human = cli(dir.path(), &["run", "index.mq.md"]);
    assert_exit(&human, 1, "human rendering also fails");
    let stderr = stderr_text(&human);
    assert!(stderr.contains("physics.mq.md"), "{stderr}");
    let primary = stderr
        .lines()
        .find(|line| line.trim_start().starts_with("at "))
        .unwrap_or_default();
    assert!(primary.contains("physics.mq.md"), "{primary}");
}

#[test]
fn platform_reports_strict_unavailable_until_security_gate_passes() {
    let root = repo_root();
    let hello = root.join("examples").join("hello.mq.md");
    let output = cli(
        &root,
        &["inspect", hello.to_str().unwrap(), "--platform", "--json"],
    );
    assert_exit(&output, 0, "inspect --platform");
    let value = json_stdout(&output);
    let strict = &value["platform"]["strict"];
    assert_eq!(
        strict["state"], "unavailable",
        "strict must stay unavailable until the ADR-0019 security gate passes: {strict}"
    );
    assert!(
        strict["reason"]
            .as_str()
            .map(|reason| !reason.trim().is_empty())
            .unwrap_or(false),
        "unavailable strict needs an actionable reason: {strict}"
    );
    assert_eq!(value["platform"]["worker_processes"], true);
}

/// A manifest whose only artifact is for another target must be rejected with
/// `no_matching_artifact` *before* any library is loaded. This test needs no C
/// compiler and runs on every platform, including Windows.
#[test]
fn plugin_foreign_target_is_rejected_without_loading() {
    let host = host_triple();
    let foreign = if host == "x86_64-unknown-linux-gnu" {
        "aarch64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let dir = TempDir::new("plugin-foreign-target");
    let file = dynamic_library_name();
    dir.write(file, "this is not a shared library");
    let digest = hex::encode(sha2::Sha256::digest(dir.read(file).as_bytes()));
    dir.write(
        "plugin.yaml",
        &format!(
            "plugin:\n  name: demo.solver\n  version: 0.1.0\n  abi: \"^1.0\"\n  kind: native\n  exports:\n    - name: solver.add\n      signature: \"($a: Int, $b: Int) -> Int\"\n  artifacts:\n    - target: {foreign}\n      file: {file}\n      sha256: \"{digest}\"\n"
        ),
    );
    let output = cli(
        dir.path(),
        &["plugin", "inspect", dir.path().to_str().unwrap(), "--json"],
    );
    assert_exit(&output, 1, "foreign artifact target must fail closed");
    let value = json_stdout(&output);
    assert_eq!(value["success"], false);
    assert_eq!(value["error"]["code"], "no_matching_artifact");
    let message = value["error"]["message"]
        .as_str()
        .expect("error message is a string");
    assert!(
        message.contains(&host),
        "the diagnostic must name the host target `{host}`: {message}"
    );
}

/// A content-hash mismatch is detected by inspection and the library is never
/// loaded. The test mutates the built artifact after the manifest was sealed.
#[test]
fn plugin_artifact_tampering_is_rejected_before_load() {
    let Some((_dir, plugin_dir)) = build_plugin_copy("plugin-tamper") else {
        return;
    };
    let artifact = plugin_dir.join(dynamic_library_name());
    let mut bytes = fs::read(&artifact).expect("read artifact");
    bytes.push(0);
    fs::write(&artifact, bytes).expect("tamper with artifact");

    let output = cli(
        repo_root().as_path(),
        &["plugin", "inspect", plugin_dir.to_str().unwrap(), "--json"],
    );
    assert_exit(&output, 1, "tampered artifact must fail closed");
    let value = json_stdout(&output);
    assert_eq!(value["success"], false);
    assert_eq!(value["error"]["code"], "hash_mismatch");
    assert!(value["error"]["message"].as_str().is_some());
}

/// An ABI major mismatch reported by the native library is rejected by
/// negotiation before any export runs.
#[test]
fn plugin_abi_major_mismatch_is_rejected_before_execution() {
    let Some((_dir, plugin_dir)) =
        build_plugin_copy_with("plugin-abi-mismatch", &["-DMQ_SOLVER_REPORT_ABI_MAJOR=2"])
    else {
        return;
    };
    let dir = TempDir::new("plugin-abi-run");
    dir.write(
        "doc.mq.md",
        "## main\n\n**结果 = solver.add(a=20, b=22)**\n\n> 打印(结果)\n",
    );
    let output = cli(
        dir.path(),
        &[
            "run",
            "doc.mq.md",
            "--plugin",
            plugin_dir.to_str().unwrap(),
            "--trust-native-plugin",
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_exit(&output, 1, "ABI major mismatch must fail before execution");
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("abi_major_mismatch"),
        "expected abi_major_mismatch, got: {stderr}"
    );
    assert!(
        !stdout_text(&output).contains("42"),
        "plugin export must not run after an ABI mismatch"
    );
}
