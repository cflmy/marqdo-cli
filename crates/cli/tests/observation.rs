//! Black-box Observation contract tests.
//!
//! These drive the real `marqdo` binary (`CARGO_BIN_EXE_marqdo`) and parse the
//! stdout line stream exactly like an external consumer. They cover the P0
//! envelope/lifecycle/byte-fidelity contract and the P1 frame/value policy,
//! plus snapshot equivalence, truncation, live delivery and a broken pipe.
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const BASIC: &str =
    "## @exec main\n\n`打印(\"START\")`\n\n`n = 10`\n\n`打印(\"DONE\")`\n\n`return n`\n";

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "marqdo-execution-events-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(dir: &Path, args: &[&str]) -> std::process::Output {
    stdlib_support::cli_command()
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run marqdo")
}

fn parse_events(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("invalid JSONL line {line:?}: {error}"))
        })
        .collect()
}

fn events_of<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

fn chunk_bytes(events: &[Value], kind: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for event in events_of(events, kind) {
        let payload = &event["payload"];
        assert_eq!(payload["encoding"], "base64");
        let data = base64_decode(payload["data_base64"].as_str().unwrap());
        assert_eq!(
            payload["byte_count"].as_u64().unwrap() as usize,
            data.len(),
            "byte_count must equal decoded length"
        );
        out.extend(data);
    }
    out
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let bytes: Vec<u8> = text
        .bytes()
        .filter(|byte| *byte != b'=')
        .map(|byte| {
            ALPHABET
                .iter()
                .position(|item| *item == byte)
                .unwrap_or_else(|| panic!("invalid base64 byte {byte}")) as u8
        })
        .collect();
    for chunk in bytes.chunks(4) {
        let mut triple = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            triple |= u32::from(*byte) << (18 - 6 * index);
        }
        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    out
}

fn assert_envelope(events: &[Value]) {
    assert!(!events.is_empty());
    let first = &events[0];
    assert_eq!(first["kind"], "stream.opened");
    assert_eq!(first["schema"], "marqdo.observation");
    assert_eq!(first["schema_version"], 1);
    assert_eq!(first["seq"], 0);
    let run_id = first["run_id"].as_str().unwrap().to_string();
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["seq"].as_u64().unwrap(), index as u64);
        assert_eq!(event["run_id"], run_id);
        assert_eq!(event["schema_version"], 1);
        assert!(event["payload"].is_object());
    }
    let terminals: Vec<&str> = events
        .iter()
        .filter_map(|event| match event["kind"].as_str().unwrap() {
            "run.completed" | "run.failed" | "run.rejected" => {
                Some(event["kind"].as_str().unwrap())
            }
            _ => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1, "exactly one terminal event");
    assert_eq!(events.last().unwrap()["kind"], terminals[0]);
    assert_eq!(events_of(events, "stream.opened").len(), 1);
    assert!(events_of(events, "run.started").len() <= 1);
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn python_consumer_available() -> bool {
    Command::new("python3")
        .args(["-c", "import jsonschema"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
fn checked_in_fixtures_validate_against_the_public_schema() {
    if !python_consumer_available() {
        eprintln!("python3 + jsonschema unavailable; schema consumer skipped");
        return;
    }
    let fixtures = repo_root().join("tests/consumers/fixtures/observation");
    let consumer = repo_root().join("tests/consumers/observation_consumer.py");
    for name in ["minimal.jsonl", "full.jsonl"] {
        let path = fixtures.join("valid").join(name);
        let output = Command::new("python3")
            .arg(&consumer)
            .arg("--validate-schema")
            .arg(&path)
            .output()
            .expect("run observation consumer");
        assert!(
            output.status.success(),
            "valid fixture {name} rejected: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    for entry in fs::read_dir(fixtures.join("invalid")).unwrap() {
        let path = entry.unwrap().path();
        let output = Command::new("python3")
            .arg(&consumer)
            .arg("--validate-schema")
            .arg(&path)
            .output()
            .expect("run observation consumer");
        assert!(
            !output.status.success(),
            "invalid fixture {} was accepted",
            path.display()
        );
    }
}

#[test]
fn structure_fixture_attributes_repeated_recursive_frames() {
    let dir = TempDir::new("structure");
    let source = fs::read_to_string(
        repo_root().join("tests/consumers/fixtures/observation/structure.mq.md"),
    )
    .unwrap();
    dir.write("index.mq.md", &source);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-max-events",
            "1000",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    assert_eq!(events.last().unwrap()["payload"]["value"]["decimal"], "3");

    let enters = events_of(&events, "call.enter");
    assert_eq!(enters.len(), 4, "main + f(2) + g(2) + f(0)");
    let frames: Vec<&str> = enters
        .iter()
        .map(|event| event["payload"]["frame_id"].as_str().unwrap())
        .collect();
    let unique: std::collections::BTreeSet<&str> = frames.iter().copied().collect();
    assert_eq!(unique.len(), 4, "frame identities are run-unique");
    let main = enters[0]["payload"]["frame_id"].as_str().unwrap();
    assert!(enters[0]["payload"]["parent_frame_id"].is_null());
    let f2 = enters[1]["payload"]["frame_id"].as_str().unwrap();
    let g2 = enters[2]["payload"]["frame_id"].as_str().unwrap();
    let f0 = enters[3]["payload"]["frame_id"].as_str().unwrap();
    assert_eq!(enters[1]["payload"]["parent_frame_id"], main);
    assert_eq!(enters[2]["payload"]["parent_frame_id"], f2);
    assert_eq!(enters[3]["payload"]["parent_frame_id"], main);
    let exit_frames: Vec<&str> = events_of(&events, "call.exit")
        .iter()
        .map(|event| event["payload"]["frame_id"].as_str().unwrap())
        .collect();
    for frame in [main, f2, g2, f0] {
        assert!(exit_frames.contains(&frame), "frame {frame} must close");
    }
    assert_eq!(events_of(&events, "branch.eval").len(), 2);
    assert_eq!(
        events_of(&events, "loop.iteration")
            .iter()
            .map(|event| event["payload"]["decision"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["continue", "continue", "exit"]
    );
}

#[test]
fn native_plugin_is_fail_closed_in_event_mode() {
    use sha2::Digest;
    let dir = TempDir::new("native-plugin");
    dir.write("index.mq.md", "## @exec main\n\n`return 1`\n");
    let plugin_dir = dir.path().join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    let artifact = plugin_dir.join("solver.so");
    fs::write(&artifact, b"not-a-real-library").unwrap();
    let digest = hex::encode(sha2::Sha256::digest(fs::read(&artifact).unwrap()));
    let manifest = format!(
        "plugin:\n  name: demo.solver\n  version: 0.1.0\n  abi: \"^1.0\"\n  kind: native\n  exports:\n    - name: solver.add\n      signature: \"($a: Int, $b: Int) -> Int\"\n  requires:\n    - math.compute\n  artifacts:\n    - target: {}\n      file: solver.so\n      sha256: \"{}\"\n",
        marqdo_host::plugin::HostTarget::current().triple(),
        digest
    );
    fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--plugin",
            plugin_dir.join("plugin.yaml").to_str().unwrap(),
            "--allow-plugin",
            "demo.solver",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    let terminal = events.last().unwrap();
    assert_eq!(terminal["kind"], "run.rejected");
    assert_eq!(
        terminal["payload"]["error"]["code"],
        "ext.observation_unsupported_native_plugin"
    );
    assert!(
        events_of(&events, "run.started").is_empty(),
        "the native plugin is rejected before execution"
    );
}

#[test]
fn selected_values_apply_equally_to_stream_and_private_snapshot() {
    let dir = TempDir::new("capture-consent");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n`secret_token = \"DO_NOT_LEAK_SENTINEL_1\"`\n\n`visible_count = 42`\n\n`return visible_count`\n",
    );
    let trace_path = dir.path().join("trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-variable",
            "secret_token",
            "--observe-text-values",
            "--observation-file",
            trace_path.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("DO_NOT_LEAK_SENTINEL_1"),
        "the explicit event consent publishes the selected value in the stream"
    );
    let snapshot_bytes = fs::read(&trace_path).unwrap();
    let snapshot: Value = serde_json::from_slice(&snapshot_bytes).unwrap();
    assert_eq!(
        snapshot["events"],
        serde_json::json!(parse_events(&output.stdout))
    );
    assert_eq!(
        snapshot["capture"]["selected_variables"],
        serde_json::json!(["secret_token"])
    );
    assert!(String::from_utf8_lossy(&snapshot_bytes).contains("DO_NOT_LEAK_SENTINEL_1"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&trace_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn basic_stream_is_pure_jsonl_and_reconstructs_program_output() {
    let dir = TempDir::new("basic");
    dir.write("index.mq.md", BASIC);
    let output = cli(dir.path(), &["run", "index.mq.md", "--output", "jsonl"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    assert_eq!(events_of(&events, "run.started").len(), 1);
    assert_eq!(chunk_bytes(&events, "stdout.chunk"), b"START\nDONE\n");
    let terminal = events.last().unwrap();
    assert_eq!(terminal["kind"], "run.completed");
    assert_eq!(terminal["payload"]["status"], "ok");
    assert_eq!(terminal["payload"]["value"]["decimal"], "10");
    // Terminal sequence is after all chunk sequences.
    let terminal_seq = terminal["seq"].as_u64().unwrap();
    for event in events_of(&events, "stdout.chunk") {
        assert!(event["seq"].as_u64().unwrap() < terminal_seq);
    }

    // The normal (non-event) run writes the same bytes and return value.
    let normal = cli(dir.path(), &["run", "index.mq.md"]);
    assert_eq!(normal.status.code(), Some(0));
    assert_eq!(normal.stdout, b"START\nDONE\n");
}

#[test]
fn retired_run_options_and_static_observation_are_rejected() {
    let dir = TempDir::new("reject");
    dir.write("index.mq.md", BASIC);
    for args in [
        vec!["--json"],
        vec!["--event-stream", "jsonl"],
        vec!["--trace-file", "retired.json"],
        vec!["--trace"],
        vec!["--output", "jsonl", "--output", "json"],
        vec!["--observe-variable", "n"],
    ] {
        let mut command = vec!["run", "index.mq.md"];
        command.extend(args);
        let rejected = cli(dir.path(), &command);
        assert_eq!(rejected.status.code(), Some(2));
        assert!(rejected.stdout.is_empty(), "{command:?} must not execute");
    }
    assert!(!dir.path().join("retired.json").exists());
    for command in ["check", "inspect"] {
        let rejected = cli(dir.path(), &[command, "index.mq.md", "--output", "jsonl"]);
        assert_eq!(rejected.status.code(), Some(2));
        assert!(rejected.stdout.is_empty(), "{command} must not execute");
    }
}

#[test]
fn prepare_and_runtime_failures_have_defined_terminals() {
    let dir = TempDir::new("failures");
    dir.write("broken.mq.md", "## @exec main\n\n`if true`\n\n`return 1`\n");
    let rejected = cli(dir.path(), &["run", "broken.mq.md", "--output", "jsonl"]);
    assert_eq!(rejected.status.code(), Some(1));
    let events = parse_events(&rejected.stdout);
    assert_envelope(&events);
    assert_eq!(events.last().unwrap()["kind"], "run.rejected");
    assert_eq!(events.last().unwrap()["payload"]["phase"], "compile");
    assert!(
        events_of(&events, "run.started").is_empty(),
        "a rejected run never starts"
    );

    dir.write(
        "runtime.mq.md",
        "## @exec main\n\n`x = 1 / 0`\n\n`return x`\n",
    );
    let failed = cli(dir.path(), &["run", "runtime.mq.md", "--output", "jsonl"]);
    assert_eq!(failed.status.code(), Some(1));
    let events = parse_events(&failed.stdout);
    assert_envelope(&events);
    assert_eq!(events.last().unwrap()["kind"], "run.failed");
    assert_eq!(
        events.last().unwrap()["payload"]["error"]["code"],
        "runtime.division_by_zero"
    );
    assert!(events_of(&events, "run.completed").is_empty());
    let exit = events_of(&events, "call.exit");
    assert_eq!(exit.len(), 1, "failure still closes the entered frame");
    assert_eq!(exit[0]["payload"]["status"], "runtime.division_by_zero");
}

#[test]
fn value_observation_is_exact_and_requires_text_consent() {
    let dir = TempDir::new("values");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n`secret_token = \"DO_NOT_LEAK_SENTINEL_1\"`\n\n`visible_count = 42`\n\n`return visible_count`\n",
    );
    // Default: metadata only; no value, no sentinel anywhere.
    let default = cli(dir.path(), &["run", "index.mq.md", "--output", "jsonl"]);
    assert_eq!(default.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&default.stdout).contains("DO_NOT_LEAK_SENTINEL_1"));
    for event in events_of(&parse_events(&default.stdout), "variable.assigned") {
        assert_eq!(event["payload"]["value_state"], "not_requested");
        assert!(event["payload"].get("value").is_none());
    }

    let selected = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-variable",
            "visible_count",
        ],
    );
    let events = parse_events(&selected.stdout);
    let visible = events_of(&events, "variable.assigned")
        .into_iter()
        .find(|event| event["payload"]["name"] == "visible_count")
        .expect("visible_count event");
    assert_eq!(visible["payload"]["value_state"], "observed");
    assert_eq!(visible["payload"]["value"]["decimal"], "42");
    // The unselected variable must never carry a value.
    for event in events_of(&events, "variable.assigned") {
        if event["payload"]["name"] == "secret_token" {
            assert_eq!(event["payload"]["value_state"], "not_requested");
            assert!(event["payload"].get("value").is_none());
        }
    }

    // Selecting the text variable is not enough: the bytes stay redacted.
    let redacted = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-variable",
            "secret_token",
        ],
    );
    assert!(!String::from_utf8_lossy(&redacted.stdout).contains("DO_NOT_LEAK_SENTINEL_1"));
    let events = parse_events(&redacted.stdout);
    let secret = events_of(&events, "variable.assigned")
        .into_iter()
        .find(|event| event["payload"]["name"] == "secret_token")
        .expect("secret_token event");
    assert_eq!(secret["payload"]["value_state"], "redacted");
    assert!(secret["payload"].get("value").is_none());

    // A second explicit consent publishes only the selected value.
    let consented = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-variable",
            "secret_token",
            "--observe-text-values",
        ],
    );
    let stdout = String::from_utf8_lossy(&consented.stdout);
    assert!(stdout.contains("DO_NOT_LEAK_SENTINEL_1"));
    let events = parse_events(&consented.stdout);
    let secret = events_of(&events, "variable.assigned")
        .into_iter()
        .find(|event| event["payload"]["name"] == "secret_token")
        .unwrap();
    assert_eq!(secret["payload"]["value_state"], "observed");
    assert_eq!(
        secret["payload"]["value"]["value"],
        "DO_NOT_LEAK_SENTINEL_1"
    );
}

#[test]
fn binary_and_multibyte_output_is_byte_exact() {
    let dir = TempDir::new("bytes");
    dir.write(
        "index.mq.md",
        "## @exec main\n\n`打印(\"开始文本\")`\n\n`流 = resource.open(\"stdio\", \"stdout\", [\"write\"])`\n\n`内存 = memory.new(1, 1)`\n\n`内存[0] = 255`\n\n`内存[1] = 0`\n\n`内存[2] = 128`\n\n`写入 = resource.write(流, 内存, 0, 3)`\n\n`关闭 = resource.close(流)`\n\n`return 写入`\n",
    );
    let normal = cli(dir.path(), &["run", "index.mq.md"]);
    assert_eq!(
        normal.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&normal.stderr)
    );
    let streamed = cli(dir.path(), &["run", "index.mq.md", "--output", "jsonl"]);
    assert_eq!(streamed.status.code(), Some(0));
    let events = parse_events(&streamed.stdout);
    let chunks = chunk_bytes(&events, "stdout.chunk");
    assert_eq!(
        chunks, normal.stdout,
        "error text may not go to stdout; concatenated chunks must equal the program bytes"
    );
    assert!(chunks.starts_with("开始文本".as_bytes()));
    assert_eq!(&chunks[chunks.len() - 3..], &[0xff, 0x00, 0x80]);
}

#[test]
fn stream_and_snapshot_share_every_event_from_one_run() {
    let dir = TempDir::new("destinations");
    dir.write("index.mq.md", BASIC);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observation-file",
            "run.json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    let snapshot: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.json")).unwrap()).unwrap();
    assert_eq!(snapshot["schema"], "marqdo.observation");
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["run"]["id"], events[0]["run_id"]);
    assert_eq!(snapshot["run"]["source"], events[1]["payload"]["source"]);
    assert_eq!(snapshot["events"], serde_json::json!(events));

    let second = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "json",
            "--observation-file",
            "run.json",
        ],
    );
    assert!(second.status.success(), "{second:?}");
    assert_eq!(
        second.stdout,
        fs::read(dir.path().join("run.json")).unwrap()
    );
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_ne!(second["run"]["id"], snapshot["run"]["id"]);
    assert_eq!(second["run"]["status"], "ok");
}

#[test]
fn structural_budget_truncates_without_losing_critical_events() {
    let dir = TempDir::new("budget");
    dir.write("index.mq.md", BASIC);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-max-events",
            "5",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    let markers = events_of(&events, "observation.truncated");
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0]["payload"]["reason"], "event_budget");
    assert_eq!(chunk_bytes(&events, "stdout.chunk"), b"START\nDONE\n");
    assert_eq!(events.last().unwrap()["kind"], "run.completed");
}

#[test]
fn output_budget_still_fails_the_run_and_emits_no_chunk() {
    let dir = TempDir::new("output-budget");
    dir.write("index.mq.md", BASIC);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--max-output",
            "3",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    assert!(chunk_bytes(&events, "stdout.chunk").is_empty());
    assert_eq!(events.last().unwrap()["kind"], "run.failed");
    assert_eq!(
        events.last().unwrap()["payload"]["error"]["code"],
        "resource.exhausted"
    );
}

#[test]
fn live_chunks_arrive_while_the_program_is_still_running() {
    let dir = TempDir::new("live");
    let source = "## @exec main\n\n`打印(\"READY\")`\n\n`i = 0`\n\n`while i < 2000000`\n\n`i = i + 1`\n\n`end`\n\n`打印(\"FINISH\")`\n\n`return i`\n";
    dir.write("index.mq.md", source);
    let mut child = stdlib_support::cli_command()
        .args([
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-max-events",
            "20",
            "--fuel",
            "100000000",
        ])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn marqdo");
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut events = Vec::new();
    let mut ready_seen_alive = false;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("read event line");
        if read == 0 {
            break;
        }
        let event: Value = serde_json::from_str(line.trim_end()).expect("event JSON");
        if event["kind"] == "stdout.chunk" {
            let data = base64_decode(event["payload"]["data_base64"].as_str().unwrap());
            if data == b"READY\n" || data == b"READY" {
                assert!(
                    child.try_wait().expect("try_wait").is_none(),
                    "READY was only readable after the process exited; this is not live output"
                );
                ready_seen_alive = true;
            }
        }
        events.push(event);
    }
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(0));
    assert!(
        ready_seen_alive,
        "the READY chunk must be readable before the process exits"
    );
    assert_eq!(chunk_bytes(&events, "stdout.chunk"), b"READY\nFINISH\n");
    assert_eq!(events.last().unwrap()["kind"], "run.completed");
}

#[test]
fn broken_pipe_makes_the_run_fail_instead_of_running_forever() {
    let dir = TempDir::new("pipe");
    let source = "## @exec main\n\n`打印(\"START\")`\n\n`i = 0`\n\n`while i < 5000000`\n\n`i = i + 1`\n\n`end`\n\n`return i`\n";
    dir.write("index.mq.md", source);
    let mut child = stdlib_support::cli_command()
        .args([
            "run",
            "index.mq.md",
            "--output",
            "jsonl",
            "--observe-max-events",
            "10",
            "--fuel",
            "100000000",
        ])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn marqdo");
    {
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).expect("read one line");
        assert!(line.contains("stream.opened"));
        // Dropping the reader closes the pipe; the next write must fail.
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                assert_ne!(
                    status.code(),
                    Some(0),
                    "a broken event pipe must fail the run"
                );
                break;
            }
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("the run kept executing after its event pipe was closed");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.contains("observe.transport_failed"),
        "expected a transport diagnostic on stderr: {stderr}"
    );
}

#[test]
fn loop_transfers_are_ordered_structural_events_with_source_spans() {
    use sha2::{Digest, Sha256};

    let dir = TempDir::new("loop-transfers");
    let source = r#"## @exec main

`索引 = 0`

`while true`

`索引 = 索引 + 1`

`if 索引 == 1`

`continue`

`else if 索引 == 2`

`break`

`end`

`end`

`return 索引`
"#;
    dir.write("index.mq.md", source);
    let checked = cli(
        dir.path(),
        &["check", "index.mq.md", "--emit", "analysis.json"],
    );
    assert!(checked.status.success(), "{checked:?}");
    let analysis_bytes = fs::read(dir.path().join("analysis.json")).unwrap();
    let analysis: Value = serde_json::from_slice(&analysis_bytes).unwrap();
    assert_eq!(analysis["schema_version"], 3);
    let module = &analysis["modules"][0];
    let branch = &module["functions"][0]["body"][1]["body"][1];
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--mode",
            "pure",
            "--output",
            "jsonl",
            "--observation-file",
            "trace.json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    assert_eq!(events.last().unwrap()["kind"], "run.completed");
    let trace: Value =
        serde_json::from_slice(&fs::read(dir.path().join("trace.json")).unwrap()).unwrap();
    assert_eq!(trace["schema_version"], 1);
    assert_eq!(
        trace["run"]["source"]["analysis_digest"],
        format!("sha256:{:x}", Sha256::digest(analysis_bytes))
    );

    let kinds = |events: &[Value]| {
        events
            .iter()
            .filter_map(|event| match event["kind"].as_str() {
                Some("loop.iteration") => Some("loop.iteration"),
                Some("control.continue") => Some("control.continue"),
                Some("control.break") => Some("control.break"),
                Some("return") => Some("return"),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let expected = [
        "loop.iteration",
        "control.continue",
        "loop.iteration",
        "control.break",
        "return",
    ];
    assert_eq!(kinds(&events), expected);
    let trace_events = trace["events"].as_array().unwrap();
    assert_eq!(trace_events, &events);
    let frame = &events_of(&events, "call.enter")[0]["payload"]["frame_id"];
    for (index, keyword) in ["continue", "break"].iter().enumerate() {
        let kind = format!("control.{keyword}");
        let carrier = format!("`{keyword}`");
        let start = source.find(&carrier).unwrap();
        let span = serde_json::json!({"start": start, "end": start + carrier.len()});
        let payload = &events_of(&events, &kind)[0]["payload"];
        assert_eq!(payload["frame_id"], *frame);
        assert_eq!(payload["module_id"], module["id"]);
        assert_eq!(payload["span"], span);
        assert_eq!(payload.as_object().unwrap().len(), 3);
        let statement = &branch["arms"][index]["body"][0];
        assert_eq!(statement["kind"], *keyword);
        assert_eq!(statement["span"], span);
    }
}

#[test]
fn strict_mode_rejects_live_and_file_destinations_before_opening_a_stream() {
    let dir = TempDir::new("strict-destinations");
    dir.write("index.mq.md", BASIC);
    let snapshot = dir.path().join("snapshot.json");
    let snapshot_arg = snapshot.to_str().unwrap().to_string();

    for args in [
        vec![
            "run",
            "index.mq.md",
            "--mode",
            "strict",
            "--output",
            "jsonl",
        ],
        vec![
            "run",
            "index.mq.md",
            "--mode",
            "strict",
            "--output",
            "text",
            "--observation-file",
            snapshot_arg.as_str(),
        ],
    ] {
        let output = cli(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(
            output.stdout.is_empty(),
            "a rejected strict destination must not emit a partial stream: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("strict.unsupported_grant"), "{stderr}");
        assert!(
            !snapshot.exists(),
            "a rejected strict destination must not publish a snapshot file"
        );
    }
}
