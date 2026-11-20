//! External CLI Tool Protocol v1 black-box tests (spec/tool-v1.md).
//!
//! The tool fixture is compiled with rustc from `tests/fixtures/tool_echo.rs`
//! rather than a shell script, so these tests run on Windows and macOS too.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-tool-test-{}-{id}-{tag}",
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

fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("MARQDO_TOOL_HOME", home)
        .output()
        .expect("run marqdo")
}

fn fixture_bytes() -> Option<&'static [u8]> {
    static BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()?
                .parent()?
                .join("tests/fixtures/tool_echo.rs");
            let output = std::env::temp_dir().join(format!(
                "marqdo-tool-fixture-{}{}",
                std::process::id(),
                std::env::consts::EXE_SUFFIX
            ));
            let status = Command::new("rustc")
                .args(["--edition", "2021", "-o"])
                .arg(&output)
                .arg(&source)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }
            let bytes = std::fs::read(&output).ok()?;
            let _ = std::fs::remove_file(&output);
            Some(bytes)
        })
        .as_deref()
}

fn write_tool(dir: &Path, command: &str, _script: &str) -> PathBuf {
    let bytes = fixture_bytes().expect("compile the portable tool fixture with rustc");
    let package = dir.join(format!("{command}-pkg"));
    std::fs::create_dir_all(package.join("bin")).unwrap();
    let entry = package
        .join("bin")
        .join(format!("{command}{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&entry, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&entry).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&entry, permissions).unwrap();
    }
    let entrypoint = format!("bin/{command}{}", std::env::consts::EXE_SUFFIX);
    let manifest = package.join("tool.json");
    std::fs::write(
        &manifest,
        format!(
            r#"{{"schema":"marqdo.cli-tool","schema_version":1,"name":"demo.{command}","version":"0.1.0","command":"{command}","entrypoint":"{entrypoint}","compatibility":{{"marqdo":">=0.1,<1"}}}}"#
        ),
    )
    .unwrap();
    manifest
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn install_dispatch_list_remove_without_core_knowledge_of_the_name() {
    let dir = TempDir::new("roundtrip");
    let home = dir.path().join("home");
    let manifest = write_tool(
        dir.path(),
        "benchmark",
        "#!/bin/sh\nfor a in \"$@\"; do printf 'arg=%s\\n' \"$a\"; done\nprintf 'protocol=%s\\n' \"$MARQDO_TOOL_PROTOCOL\"\nexit 7\n",
    );

    let install = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
    assert_eq!(install.status.code(), Some(0), "{}", stderr(&install));

    let listed = cli(&home, &["tool", "list"]);
    assert!(stdout(&listed).contains("benchmark 0.1.0"));

    let output = cli(&home, &["benchmark", "run", ".", "空格"]);
    assert_eq!(output.status.code(), Some(7));
    let text = stdout(&output);
    assert!(text.contains("arg=run"));
    assert!(text.contains("arg=."));
    assert!(text.contains("arg=空格"));
    assert!(text.contains("protocol=1"));

    let removed = cli(&home, &["tool", "remove", "benchmark"]);
    assert_eq!(removed.status.code(), Some(0));
    let unknown = cli(&home, &["benchmark"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(stderr(&unknown).contains("unknown command"));
}

#[test]
fn invalid_manifests_and_path_escapes_are_rejected() {
    let dir = TempDir::new("invalid");
    let home = dir.path().join("home");
    let manifest = write_tool(dir.path(), "lintx", "#!/bin/sh\nexit 0\n");

    let builtin = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"command\":\"lintx\"", "\"command\":\"run\"");
    let builtin_manifest = dir.path().join("builtin.json");
    std::fs::write(&builtin_manifest, builtin).unwrap();
    let output = cli(
        &home,
        &["tool", "install", builtin_manifest.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("built-in"));

    let traversal = std::fs::read_to_string(&manifest).unwrap().replace(
        &format!("bin/lintx{}", std::env::consts::EXE_SUFFIX),
        "../outside",
    );
    let traversal_manifest = dir.path().join("traversal.json");
    std::fs::write(&traversal_manifest, traversal).unwrap();
    let output = cli(
        &home,
        &["tool", "install", traversal_manifest.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));

    let incompatible = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace(">=0.1,<1", ">=99.0");
    let incompatible_manifest = dir.path().join("incompatible.json");
    std::fs::write(&incompatible_manifest, incompatible).unwrap();
    let output = cli(
        &home,
        &["tool", "install", incompatible_manifest.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("requires marqdo"));
}

#[test]
fn tampered_binary_and_path_hijack_are_not_executed() {
    let dir = TempDir::new("security");
    let home = dir.path().join("home");
    let manifest = write_tool(dir.path(), "custom-abc", "#!/bin/sh\nexit 0\n");
    assert_eq!(
        cli(&home, &["tool", "install", manifest.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );
    let installed = home
        .join("custom-abc")
        .join("bin")
        .join(format!("custom-abc{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&installed, "#!/bin/sh\nexit 99\n").unwrap();
    let tampered = cli(&home, &["custom-abc"]);
    assert_eq!(tampered.status.code(), Some(1));
    assert!(stderr(&tampered).contains("hash mismatch"));

    // A PATH program with the marqdo prefix is not implicitly a tool.
    let path_dir = dir.path().join("path");
    std::fs::create_dir_all(&path_dir).unwrap();
    let hijack = path_dir.join(format!("marqdo-hijack{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&hijack, b"not a registered tool").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&hijack).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&hijack, permissions).unwrap();
    }
    let output = Command::new(binary())
        .arg("hijack")
        .env("MARQDO_TOOL_HOME", &home)
        .env("PATH", &path_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown command"));
}

#[test]
fn protocol_negotiation_accepts_declared_versions_and_rejects_disjoint_ones() {
    let dir = TempDir::new("protocols");
    let home = dir.path().join("home");
    let manifest = write_tool(
        dir.path(),
        "proto",
        "#!/bin/sh\nprintf 'analysis=%s\\ntrace=%s\\n' \"$MARQDO_PROTOCOL_ANALYSIS\" \"$MARQDO_PROTOCOL_TRACE\"\n",
    );
    let text = std::fs::read_to_string(&manifest).unwrap().replace(
        "\"marqdo\":\">=0.1,<1\"",
        "\"marqdo\":\">=0.1,<1\",\"analysis\":[2],\"trace\":[2]",
    );
    std::fs::write(&manifest, text).unwrap();
    let install = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
    assert_eq!(install.status.code(), Some(0), "{}", stderr(&install));
    let output = cli(&home, &["proto"]);
    assert_eq!(output.status.code(), Some(7), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("analysis=2"), "{text}");
    assert!(text.contains("trace=2"), "{text}");

    for (compat, tag) in [
        ("\"analysis\":[99]", "analysis-disjoint"),
        ("\"analysis\":[]", "analysis-empty"),
        ("\"trace\":[9]", "trace-disjoint"),
    ] {
        let command = format!("bad-{tag}");
        let manifest = write_tool(dir.path(), &command, "#!/bin/sh\nexit 0\n");
        let text = std::fs::read_to_string(&manifest).unwrap().replace(
            "\"marqdo\":\">=0.1,<1\"",
            &format!("\"marqdo\":\">=0.1,<1\",{compat}"),
        );
        std::fs::write(&manifest, text).unwrap();
        let output = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1), "{tag}");
        assert!(
            stderr(&output).contains("protocol_incompatible"),
            "{tag}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn tampering_any_bundle_file_or_adding_an_unlisted_file_is_rejected() {
    let dir = TempDir::new("bundle");
    let home = dir.path().join("home");
    let manifest = write_tool(dir.path(), "bundletool", "#!/bin/sh\nprintf 'entry\\n'\n");
    let package = manifest.parent().unwrap();
    std::fs::create_dir_all(package.join("lib")).unwrap();
    std::fs::write(package.join("lib/helper.sh"), "echo helper\n").unwrap();

    let install = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
    assert_eq!(install.status.code(), Some(0), "{}", stderr(&install));
    let installed = home.join("bundletool");

    // Tamper with a non-entry file only.
    std::fs::write(installed.join("lib/helper.sh"), "echo tampered\n").unwrap();
    let run = cli(&home, &["bundletool"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(
        stderr(&run).contains("bundle hash mismatch"),
        "{}",
        stderr(&run)
    );

    // Reinstall, then add an unlisted file.
    assert_eq!(
        cli(&home, &["tool", "remove", "bundletool"]).status.code(),
        Some(0)
    );
    let install = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
    assert_eq!(install.status.code(), Some(0));
    std::fs::write(installed.join("lib/extra.sh"), "echo extra\n").unwrap();
    let run = cli(&home, &["bundletool"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(
        stderr(&run).contains("bundle hash mismatch"),
        "{}",
        stderr(&run)
    );
}

#[test]
fn a_root_record_json_in_the_source_package_is_rejected() {
    let dir = TempDir::new("reserved-record");
    let home = dir.path().join("home");
    let manifest = write_tool(dir.path(), "reserved", "#!/bin/sh\nexit 0\n");
    let package = manifest.parent().unwrap();
    std::fs::write(package.join("record.json"), "{}").unwrap();
    let output = cli(&home, &["tool", "install", manifest.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("must not contain a root record.json"));
}
