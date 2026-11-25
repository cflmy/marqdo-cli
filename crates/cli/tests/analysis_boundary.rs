//! Public Analysis boundary tests (issue #10, ADR-0069).
//!
//! These drive the real CLI binary: exact source/AST/scope projection, explicit
//! reference states, output safety, Observation digest binding, independent
//! consumers and schema/semantic mutation rejection.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use sha2::{Digest, Sha256};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-analysis-boundary-{}-{id}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, source).unwrap();
        path
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
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

fn emit(dir: &Path, entry: &str, output: &str, extra: &[&str]) -> Output {
    let mut args = vec!["check", entry, "--emit", output];
    args.extend_from_slice(extra);
    cli(dir, &args)
}

fn analysis(dir: &Path, name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap()
}

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

const DOCUMENT: &str = "\u{feff}# @knowledge 数学\r\n\r\n## @exec 计算\r\n\r\n`params(n: Int)`\r\n\r\n`return n + 1`\r\n\r\n### 细节\r\n\r\n正文\r\n";

#[test]
fn analysis_projects_exact_source_document_and_complete_scopes() {
    let dir = TempDir::new("scope");
    dir.write("index.mq.md", DOCUMENT);
    let output = emit(dir.path(), "index.mq.md", "analysis.json", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = analysis(dir.path(), "analysis.json");
    let module = &value["modules"][0];
    assert_eq!(module["source"], DOCUMENT);
    assert_eq!(module["source_digest"], digest(DOCUMENT.as_bytes()));
    assert_eq!(value["project"]["entry_module_id"], module["id"]);

    let document = &module["document"];
    assert_eq!(document["kind"], "document");
    assert_eq!(document["span"]["start"], 0);
    assert_eq!(
        document["span"]["end"].as_u64().unwrap() as usize,
        DOCUMENT.len()
    );
    let heading = document["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "heading" && node["level"] == 1)
        .expect("level-1 heading");
    assert!(heading["children"].is_array());
    assert!(heading["children"].as_array().unwrap().iter().any(|node| {
        node["kind"] == "text"
            && node["literal"]
                .as_str()
                .is_some_and(|literal| literal.contains("数学"))
    }));

    let units = module["units"].as_array().unwrap();
    let by_name = |name: &str| {
        units
            .iter()
            .find(|unit| unit["name"] == name)
            .unwrap_or_else(|| panic!("missing unit {name}"))
    };
    let root = &units[0];
    assert_eq!(root["kind"], "root");
    assert_eq!(root["parent_id"], serde_json::Value::Null);
    assert_eq!(root["heading_span"], serde_json::Value::Null);
    assert_eq!(root["span"]["start"], 0);
    assert_eq!(
        root["span"]["end"].as_u64().unwrap() as usize,
        DOCUMENT.len()
    );

    let namespace = by_name("数学");
    assert_eq!(namespace["kind"], "namespace");
    assert_eq!(namespace["parent_id"], root["id"]);
    assert_eq!(namespace["mode"], "knowledge");
    assert_eq!(namespace["mode_origin"], "explicit");
    assert_eq!(namespace["mode_source_unit_id"], namespace["id"]);

    let function = by_name("计算");
    assert_eq!(function["kind"], "function");
    assert_eq!(function["parent_id"], namespace["id"]);
    assert_eq!(function["mode"], "exec");
    assert_eq!(function["mode_origin"], "explicit");
    assert!(function["function"].is_string());
    assert_ne!(function["span"], function["heading_span"]);

    let section = by_name("细节");
    assert_eq!(section["kind"], "section");
    assert_eq!(section["parent_id"], function["id"]);
    assert_eq!(section["mode"], "exec");
    assert_eq!(section["mode_origin"], "inherited");
    assert_eq!(section["mode_source_unit_id"], function["id"]);
    // The complete function range includes the H3 child and runs to EOF.
    assert_eq!(
        function["span"]["end"].as_u64().unwrap() as usize,
        DOCUMENT.len()
    );
    assert_eq!(
        section["span"]["end"].as_u64().unwrap() as usize,
        DOCUMENT.len()
    );
    let heading_start = DOCUMENT.find("## @exec 计算").unwrap();
    assert_eq!(
        function["heading_span"]["start"].as_u64().unwrap() as usize,
        heading_start
    );

    let anchors = module["anchors"].as_array().unwrap();
    assert!(anchors.iter().any(|anchor| anchor["anchor"] == "计算"));
    let signature = &module["functions"][0]["signature"];
    assert_eq!(signature["kind"], "explicit");
    assert_eq!(signature["params"][0]["name"], "n");
    assert_eq!(signature["params"][0]["type"], "Int");
}

#[test]
fn reference_states_are_distinct_and_explicit_refs_do_not_change_the_program() {
    let dir = TempDir::new("references");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n[本地](./other.mq.md#公式) [外链](https://example.com) [本文](#main)\n\n`return 1`\n",
    );
    dir.write("other.mq.md", "## @knowledge 公式\n\n正文\n");
    let ordinary = emit(dir.path(), "index.mq.md", "ordinary.json", &[]);
    assert!(
        ordinary.status.success(),
        "{}",
        String::from_utf8_lossy(&ordinary.stderr)
    );
    let value = analysis(dir.path(), "ordinary.json");
    assert_eq!(value["modules"].as_array().unwrap().len(), 1);
    let references = value["modules"][0]["references"].as_array().unwrap();
    let state = |url: &str| {
        references
            .iter()
            .find(|reference| reference["url"] == url)
            .unwrap_or_else(|| panic!("missing reference {url}"))["state"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(state("./other.mq.md#公式"), "not_loaded");
    assert_eq!(state("https://example.com"), "external");
    assert_eq!(state("#main"), "resolved");
    // The ordinary artifact still describes only the loaded program.
    assert_eq!(value["modules"].as_array().unwrap().len(), 1);

    // Explicit refs verify the direct target and keep the same ordinary bytes'
    // module set. The emitted Analysis still uses the loaded catalog.
    let verified = emit(dir.path(), "index.mq.md", "verified.json", &["--refs"]);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verified_value = analysis(dir.path(), "verified.json");
    assert_eq!(verified_value["modules"].as_array().unwrap().len(), 1);
    let verified_refs = verified_value["modules"][0]["references"]
        .as_array()
        .unwrap();
    assert_eq!(
        verified_refs
            .iter()
            .find(|reference| reference["url"] == "./other.mq.md#公式")
            .unwrap()["state"],
        "not_loaded",
        "the emitted artifact must keep the loaded catalog"
    );
    let inspected = cli(dir.path(), &["inspect", "index.mq.md", "--refs", "--json"]);
    assert!(inspected.status.success());
    let inspected: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    let resolved = inspected["refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reference| reference["url"] == "./other.mq.md#公式")
        .unwrap();
    assert_eq!(resolved["state"], "resolved");
    assert_eq!(resolved["target_kind"], "module");
    assert!(resolved["target_module"]
        .as_str()
        .unwrap()
        .ends_with("::other.mq.md"));

    // A broken explicitly verified target is an error and publishes no artifact.
    dir.write(
        "broken.mq.md",
        "## @exec main\n\n[坏](./other.mq.md#不存在)\n\n`return 1`\n",
    );
    let broken = emit(dir.path(), "broken.mq.md", "broken.json", &["--refs"]);
    assert_eq!(broken.status.code(), Some(1), "{broken:?}");
    assert!(!dir.path().join("broken.json").exists());
    assert!(String::from_utf8_lossy(&broken.stderr).contains("ref.unresolved"));
}

#[test]
fn emit_refuses_inputs_symlinks_hardlinks_and_directories() {
    let dir = TempDir::new("emit-safety");
    let source = "## @exec main\n\n`return 1`\n";
    let entry = dir.write("index.mq.md", source);

    let self_emit = emit(dir.path(), "index.mq.md", "index.mq.md", &[]);
    assert_eq!(self_emit.status.code(), Some(2), "{self_emit:?}");
    assert_eq!(dir.read("index.mq.md"), source);

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&entry, dir.path().join("link.json")).unwrap();
        let symlink_emit = emit(dir.path(), "index.mq.md", "link.json", &[]);
        assert_eq!(symlink_emit.status.code(), Some(2), "{symlink_emit:?}");
        assert_eq!(dir.read("index.mq.md"), source);

        std::fs::hard_link(&entry, dir.path().join("hard.json")).unwrap();
        let hard_emit = emit(dir.path(), "index.mq.md", "hard.json", &[]);
        assert_eq!(hard_emit.status.code(), Some(2), "{hard_emit:?}");
        assert_eq!(dir.read("index.mq.md"), source);
    }

    std::fs::create_dir(dir.path().join("outdir")).unwrap();
    let directory_emit = emit(dir.path(), "index.mq.md", "outdir", &[]);
    assert_eq!(directory_emit.status.code(), Some(2), "{directory_emit:?}");

    let ok = emit(dir.path(), "index.mq.md", "analysis.json", &[]);
    assert!(ok.status.success(), "{ok:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("analysis.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "Analysis output must be owner-only");
    }
}

#[test]
fn observation_binds_the_exact_emitted_analysis_bytes() {
    let dir = TempDir::new("digest");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n`params(n: Int)`\n\n`return n + 1`\n",
    );
    let emitted = emit(dir.path(), "index.mq.md", "analysis.json", &[]);
    assert!(
        emitted.status.success(),
        "{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let bytes = std::fs::read(dir.path().join("analysis.json")).unwrap();
    let expected = format!("sha256:{}", digest(&bytes));

    let run = cli(
        dir.path(),
        &[
            "call",
            "index.mq.md",
            "main",
            "--arg",
            "n=1",
            "--output",
            "json",
            "--observation-file",
            "observation.json",
        ],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let observation = analysis(dir.path(), "observation.json");
    let started = observation["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "run.started")
        .expect("run.started");
    assert_eq!(started["payload"]["source"]["analysis_digest"], expected);
}

#[test]
fn independent_view_lint_and_knowledge_consumers_drive_the_real_cli() {
    let dir = TempDir::new("consumers");
    dir.write(
        "marqdo.yaml",
        "project:\n  name: consumers\n  version: 0.1.0\n",
    );
    dir.write("theory.mq.md", "## @knowledge 公式\n\n正文\n");
    dir.write(
        "index.mq.md",
        "---\nimport:\n  theory: \"./theory.mq.md\"\n---\n\n## @exec main\n\n[公式](./theory.mq.md#公式) [未加载](./notes.mq.md)\n\n`return 1`\n",
    );
    dir.write("notes.mq.md", "## @exec main\n\n`return 2`\n");
    assert!(emit(dir.path(), "index.mq.md", "index.json", &[])
        .status
        .success());
    assert!(emit(dir.path(), "notes.mq.md", "notes.json", &[])
        .status
        .success());

    let consumers = repo_root().join("tests/consumers");
    let run = |script: &str, args: &[&Path]| -> Output {
        let mut command = Command::new("python3");
        command.arg(consumers.join(script));
        for arg in args {
            command.arg(arg);
        }
        command.output().expect("python3 consumer")
    };
    let index_json = dir.path().join("index.json");
    let notes_json = dir.path().join("notes.json");

    let view = run("view_consumer.py", &[&index_json]);
    assert!(
        view.status.success(),
        "{}",
        String::from_utf8_lossy(&view.stderr)
    );
    assert!(String::from_utf8_lossy(&view.stdout).contains("view: rendered=1"));

    let lint = run("lint_consumer.py", &[&index_json]);
    assert!(
        lint.status.success(),
        "{}",
        String::from_utf8_lossy(&lint.stderr)
    );
    let lint_text = String::from_utf8_lossy(&lint.stdout);
    assert!(lint_text.contains("state=not_loaded"));
    assert!(lint_text.contains("unresolved=1"));

    let knowledge = run("knowledge_consumer.py", &[&index_json, &notes_json]);
    assert!(
        knowledge.status.success(),
        "{}",
        String::from_utf8_lossy(&knowledge.stderr)
    );
    let knowledge_text = String::from_utf8_lossy(&knowledge.stdout);
    assert!(knowledge_text.contains("entries=2"));
    assert!(knowledge_text.contains("not_loaded=1"));
}

#[test]
fn protocol_validator_accepts_current_bytes_and_rejects_mutations() {
    let dir = TempDir::new("protocol-validator");
    dir.write(
        "index.mq.md",
        "---\nimport:\n  theory: \"./theory.mq.md\"\n---\n\n## @exec main\n\n$$x^2$$\n\n```python\nprint(1)\n```\n\n[公式](./theory.mq.md#公式)\n\n`结果 = theory.加一(1)`\n\n`return 结果`\n",
    );
    dir.write(
        "theory.mq.md",
        "## @exec 加一\n\n`params(n: Int)`\n\n`return n + 1`\n\n## @knowledge 公式\n\n正文\n",
    );
    let check = emit(dir.path(), "index.mq.md", "analysis.json", &[]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let run = cli(
        dir.path(),
        &[
            "call",
            "index.mq.md",
            "main",
            "--output",
            "json",
            "--observation-file",
            "observation.json",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let validator = repo_root().join("tests/consumers/validate_protocols.py");
    let output = Command::new("python3")
        .arg(&validator)
        .arg(dir.path().join("analysis.json"))
        .arg(dir.path().join("observation.json"))
        .arg(repo_root().join("examples/tool-unknown-name/tool.json"))
        .output()
        .expect("python3 protocol validator");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("validated analysis"));
}

#[test]
fn two_entries_keep_their_own_snapshot_context() {
    let dir = TempDir::new("entries");
    for (name, body) in [("first", "1"), ("second", "2")] {
        dir.write(
            &format!("{name}.mq.md"),
            &format!("## @exec main\n\n`return {body}`\n"),
        );
        let output = emit(
            dir.path(),
            &format!("{name}.mq.md"),
            &format!("{name}.json"),
            &[],
        );
        assert!(output.status.success(), "{output:?}");
    }
    let first = analysis(dir.path(), "first.json");
    let second = analysis(dir.path(), "second.json");
    assert!(first["project"]["entry_module_id"]
        .as_str()
        .unwrap()
        .ends_with("::first.mq.md"));
    assert!(second["project"]["entry_module_id"]
        .as_str()
        .unwrap()
        .ends_with("::second.mq.md"));
    assert_ne!(first["project"]["root_id"], second["project"]["root_id"]);
}

#[test]
fn explicit_refs_verify_plain_files_reject_symlinks_and_never_guess_coverage() {
    let dir = TempDir::new("refs-files");
    dir.write("marqdo.yaml", "project:\n  name: files\n  version: 0.1.0\n");
    dir.write("asset.pdf", "%PDF-1.4 not a program\n");
    dir.write("notes.md", "# Notes\n\nplain markdown\n");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n[资产](./asset.pdf) [普通](./notes.md#notes) [越界](./%2e%2e/secret.mq.md)\n\n`return 1`\n",
    );
    #[cfg(unix)]
    let _symlink = {
        std::os::unix::fs::symlink("/etc/hostname", dir.path().join("escape.mq.md")).unwrap();
        dir.write(
            "symlink.mq.md",
            "## @exec main\n\n[逃逸](./escape.mq.md)\n\n`return 1`\n",
        );
    };
    #[cfg(not(unix))]
    let _symlink = ();

    let inspected = cli(dir.path(), &["inspect", "index.mq.md", "--refs", "--json"]);
    assert!(inspected.status.success(), "{inspected:?}");
    let value: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    let refs = value["refs"].as_array().unwrap();
    let by_url = |url: &str| {
        refs.iter()
            .find(|reference| reference["url"] == url)
            .unwrap_or_else(|| panic!("missing {url}"))
    };
    assert_eq!(by_url("./asset.pdf")["state"], "resolved");
    assert_eq!(by_url("./asset.pdf")["target_kind"], "file");
    assert_eq!(by_url("./notes.md#notes")["state"], "not_loaded");
    assert_eq!(by_url("./%2e%2e/secret.mq.md")["state"], "outside_scope");
    let issues = value["reference_issues"].as_array().unwrap();
    assert!(issues.iter().any(|issue| issue["code"] == "ref.invalid"));
    assert!(issues.iter().any(|issue| issue["code"] == "ref.not_loaded"));

    #[cfg(unix)]
    {
        let symlinked = cli(
            dir.path(),
            &["inspect", "symlink.mq.md", "--refs", "--json"],
        );
        assert!(symlinked.status.success(), "{symlinked:?}");
        let value: serde_json::Value = serde_json::from_slice(&symlinked.stdout).unwrap();
        assert_eq!(value["refs"][0]["state"], "not_loaded");
        assert!(value["reference_issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "ref.invalid"));
    }
}

#[test]
fn missing_unimported_program_is_an_explicit_refs_error_but_not_loaded_otherwise() {
    let dir = TempDir::new("refs-missing");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n[不存在](./absent.mq.md#x)\n\n`return 1`\n",
    );
    let ordinary = emit(dir.path(), "index.mq.md", "ordinary.json", &[]);
    assert!(ordinary.status.success(), "{ordinary:?}");
    let analysis = analysis(dir.path(), "ordinary.json");
    assert_eq!(
        analysis["modules"][0]["references"][0]["state"],
        "not_loaded"
    );
    let refs = emit(dir.path(), "index.mq.md", "refs.json", &["--refs"]);
    assert_eq!(refs.status.code(), Some(1), "{refs:?}");
    assert!(!dir.path().join("refs.json").exists());
    assert!(String::from_utf8_lossy(&refs.stderr).contains("ref.unresolved"));
}

#[test]
fn lock_digest_binds_the_prepared_lock_snapshot() {
    let dir = TempDir::new("lock-digest");
    dir.write("marqdo.yaml", "project:\n  name: lock\n  version: 0.1.0\n");
    dir.write("index.mq.md", "## @exec main\n\n`return 1`\n");
    let lock = cli(dir.path(), &["lock"]);
    assert!(lock.status.success(), "{lock:?}");
    let locked = emit(dir.path(), "index.mq.md", "locked.json", &[]);
    assert!(locked.status.success(), "{locked:?}");
    let lock_bytes = std::fs::read(dir.path().join("marqdo.lock")).unwrap();
    let analysis = analysis(dir.path(), "locked.json");
    assert_eq!(
        analysis["project"]["lock_digest"],
        format!("sha256:{}", digest(&lock_bytes))
    );
    let manifest_bytes = std::fs::read(dir.path().join("marqdo.yaml")).unwrap();
    assert_eq!(
        analysis["project"]["manifest_digest"],
        format!("sha256:{}", digest(&manifest_bytes))
    );

    // A lock that no longer validates cannot leave a success artifact behind.
    std::fs::write(dir.path().join("marqdo.lock"), "lock_version: 4\n").unwrap();
    let failed = emit(dir.path(), "index.mq.md", "locked.json", &[]);
    assert_ne!(failed.status.code(), Some(0), "{failed:?}");
    assert!(!dir.path().join("locked.json").exists());
}
