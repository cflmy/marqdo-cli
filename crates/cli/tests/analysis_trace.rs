//! Public Analysis/Trace v1 black-box contract (spec/analysis-v1.md,
//! spec/trace-v1.md).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-protocol-test-{}-{id}-{tag}",
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

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

const PROGRAM: &str = r#"# 数学

## 加一

`(n)`

*n + 1*

## main

**结果 = 加一(41)**

> 打印("secret-value")

*结果*
"#;

#[test]
fn analysis_is_deterministic_and_has_stable_semantic_nodes() {
    let dir = TempDir::new("analysis");
    write(dir.path(), "index.mq.md", PROGRAM);
    let first = dir.path().join("first.json");
    let second = dir.path().join("second.json");
    for path in [&first, &second] {
        let output = cli(
            dir.path(),
            &[
                "check",
                "index.mq.md",
                "--refs",
                "--emit",
                path.to_str().unwrap(),
            ],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let first_bytes = std::fs::read(&first).unwrap();
    let second_bytes = std::fs::read(&second).unwrap();
    assert_eq!(first_bytes, second_bytes, "analysis must be deterministic");

    let analysis: serde_json::Value = serde_json::from_slice(&first_bytes).unwrap();
    assert_eq!(analysis["schema"], "marqdo.analysis");
    assert_eq!(analysis["schema_version"], 2);
    let module = &analysis["modules"][0];
    assert_eq!(module["id"], "root:default::index.mq.md");
    assert_eq!(module["package_instance_id"], "root:default");
    assert!(
        analysis["project"]["root_id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sha256:"),
        "standalone documents use a workspace snapshot identity"
    );
    assert_eq!(analysis["project"]["identity_kind"], "workspace_snapshot");
    assert_eq!(module["functions"].as_array().unwrap().len(), 2);
    let main = module["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|function| function["local_name"] == "main")
        .unwrap();
    let body = main["body"].as_array().unwrap();
    assert_eq!(body.last().unwrap()["kind"], "return");
    let calls = module["calls"].as_array().unwrap();
    assert!(calls.iter().any(|call| call["state"] == "function"
        && call["target"] == "root:default::index.mq.md::数学.加一"));

    // Independent Python consumer, if available.
    let consumer = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/consumers/analysis_consumer.py");
    if let Ok(output) = Command::new("python3").arg(&consumer).arg(&first).output() {
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("functions="));
    }
}

#[test]
fn analysis_v2_identity_is_location_independent() {
    let first = TempDir::new("analysis-identity-a");
    let second = TempDir::new("analysis-identity-b");
    let source = "## main\n\n**x = 1**\n\n*x*\n";
    for dir in [&first, &second] {
        write(
            dir.path(),
            "marqdo.yaml",
            "project:\n  name: doc\n  version: 0.1.0\n",
        );
        write(dir.path(), "index.mq.md", source);
    }
    let mut bytes = Vec::new();
    for dir in [&first, &second] {
        let target = dir.path().join("analysis.json");
        let output = cli(
            dir.path(),
            &["check", "index.mq.md", "--emit", target.to_str().unwrap()],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        bytes.push(std::fs::read(&target).unwrap());
    }
    assert_eq!(
        bytes[0], bytes[1],
        "moving a project must not change Analysis bytes"
    );
    let text = String::from_utf8(bytes[0].clone()).unwrap();
    assert!(
        !text.contains(first.path().to_str().unwrap()),
        "machine path leaked into Analysis"
    );
    let analysis: serde_json::Value = serde_json::from_slice(&bytes[0]).unwrap();
    assert_eq!(analysis["project"]["root_id"], "root:doc@0.1.0");
    assert_eq!(analysis["project"]["identity_kind"], "manifest");
}

#[test]
fn analysis_v2_projects_addressable_formula_and_code_resources() {
    let dir = TempDir::new("analysis-resources");
    let source = r#"## calc

`(x)`

$$A = x + 1$$

```python
print(1)
```

*x*
"#;
    write(
        dir.path(),
        "marqdo.yaml",
        "project:\n  name: res\n  version: 0.1.0\n",
    );
    write(dir.path(), "index.mq.md", source);
    let target = dir.path().join("analysis.json");
    let output = cli(
        dir.path(),
        &["check", "index.mq.md", "--emit", target.to_str().unwrap()],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let analysis: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
    let resources = analysis["modules"][0]["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 2, "{resources:?}");
    let formula = resources
        .iter()
        .find(|resource| resource["kind"] == "formula")
        .unwrap();
    let code = resources
        .iter()
        .find(|resource| resource["kind"] == "code")
        .unwrap();
    assert_eq!(
        formula["id"],
        "root:res@0.1.0::index.mq.md#resource:formula-1"
    );
    assert_eq!(code["id"], "root:res@0.1.0::index.mq.md#resource:code-1");
    assert_eq!(formula["module_id"], "root:res@0.1.0::index.mq.md");
    assert_eq!(formula["language"], serde_json::Value::Null);
    assert_eq!(code["language"], "python");
    assert_eq!(formula["content_digest"].as_str().unwrap().len(), 64);
    assert_eq!(code["content_digest"].as_str().unwrap().len(), 64);
    assert!(
        !analysis.to_string().contains("A = x + 1"),
        "resource payloads stay in source, not Analysis"
    );
}

#[test]
fn trace_v2_budget_truncates_long_loops_without_failing_the_run() {
    let dir = TempDir::new("trace-budget");
    write(
        dir.path(),
        "index.mq.md",
        "## main\n\n**i = 0**\n\n- `i < 500`\n\n  **i = i + 1**\n\n*i*\n",
    );
    let trace_path = dir.path().join("trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--trace-file",
            trace_path.to_str().unwrap(),
            "--trace-max-events",
            "20",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&trace_path).unwrap()).unwrap();
    assert_eq!(trace["schema_version"], 2);
    assert_eq!(trace["run"]["status"], "ok");
    assert_eq!(trace["limits"]["max_events"], 20);
    assert_eq!(trace["truncated"]["reason"], "event_budget");
    assert_eq!(trace["truncated"]["limit"], 20);
    let events = trace["events"].as_array().unwrap();
    assert!(events.len() <= 21, "events: {}", events.len());
    assert_eq!(events.last().unwrap()["kind"], "trace.truncated");
    let mut previous = None;
    for event in events {
        let seq = event["seq"].as_u64().unwrap();
        if let Some(previous) = previous {
            assert!(seq > previous);
        }
        previous = Some(seq);
    }
}

#[test]
fn failed_check_removes_stale_analysis() {
    let dir = TempDir::new("analysis-error");
    write(dir.path(), "bad.mq.md", "## main\n\n1. 没有条件\n");
    let target = dir.path().join("analysis.json");
    std::fs::write(&target, "stale").unwrap();
    let output = cli(
        dir.path(),
        &["check", "bad.mq.md", "--emit", target.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!target.exists(), "stale success artifact must be removed");
}

#[test]
fn trace_records_calls_effects_and_status_without_secrets() {
    let dir = TempDir::new("trace");
    write(dir.path(), "index.mq.md", PROGRAM);
    let trace_path = dir.path().join("trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--trace-file",
            trace_path.to_str().unwrap(),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(&trace_path).unwrap();
    assert!(
        !text.contains("secret-value"),
        "values must not be recorded"
    );
    let trace: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(trace["schema"], "marqdo.trace");
    assert_eq!(trace["schema_version"], 2);
    assert_eq!(trace["run"]["status"], "ok");
    assert_eq!(trace["run"]["entry"], "root:default::index.mq.md::main");
    let events = trace["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|event| event["kind"] == "call.enter"
            && event["module_id"] == "root:default::index.mq.md"));
    assert!(events.iter().any(|event| event["kind"] == "effect"));
    assert!(events.iter().any(|event| event["kind"] == "call.exit"));
    let mut previous = None;
    for event in events {
        let seq = event["seq"].as_u64().unwrap();
        if let Some(previous) = previous {
            assert!(seq > previous);
        }
        previous = Some(seq);
    }

    // Error runs still write a terminal trace.
    write(dir.path(), "bad.mq.md", "## main\n\n**x = 1 // 0**\n");
    let error_trace = dir.path().join("error-trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "bad.mq.md",
            "--trace-file",
            error_trace.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let trace: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&error_trace).unwrap()).unwrap();
    assert_eq!(trace["run"]["status"], "runtime.division_by_zero");
}
