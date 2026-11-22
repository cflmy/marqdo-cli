//! Black-box Execution Events v1 contract tests (RFC-OBS-0001).
//!
//! These drive the real `marqdo` binary (`CARGO_BIN_EXE_marqdo`) and parse the
//! stdout line stream exactly like an external consumer. They cover the P0
//! envelope/lifecycle/byte-fidelity contract and the P1 frame/value policy,
//! plus trace coexistence, truncation, live delivery and a broken pipe.

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
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
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
    assert_eq!(first["schema"], "marqdo.execution-events");
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

#[test]
fn basic_stream_is_pure_jsonl_and_reconstructs_program_output() {
    let dir = TempDir::new("basic");
    dir.write("index.mq.md", BASIC);
    let output = cli(
        dir.path(),
        &["run", "index.mq.md", "--event-stream", "jsonl"],
    );
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
fn event_stream_rejects_mixed_json_and_static_commands() {
    let dir = TempDir::new("reject");
    dir.write("index.mq.md", BASIC);
    let mixed = cli(
        dir.path(),
        &["run", "index.mq.md", "--event-stream", "jsonl", "--json"],
    );
    assert_eq!(mixed.status.code(), Some(2));
    assert!(mixed.stdout.is_empty(), "no mixed stdout");
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("--json"));

    for command in ["check", "inspect"] {
        let rejected = cli(
            dir.path(),
            &[command, "index.mq.md", "--event-stream", "jsonl"],
        );
        assert_eq!(
            rejected.status.code(),
            Some(2),
            "{command} must reject the option"
        );
        assert!(rejected.stdout.is_empty(), "{command} must not execute");
    }
}

#[test]
fn prepare_and_runtime_failures_have_defined_terminals() {
    let dir = TempDir::new("failures");
    dir.write("broken.mq.md", "## @exec main\n\n`if true`\n\n`return 1`\n");
    let rejected = cli(
        dir.path(),
        &["run", "broken.mq.md", "--event-stream", "jsonl"],
    );
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
    let failed = cli(
        dir.path(),
        &["run", "runtime.mq.md", "--event-stream", "jsonl"],
    );
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
    let default = cli(
        dir.path(),
        &["run", "index.mq.md", "--event-stream", "jsonl"],
    );
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
            "--event-stream",
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
            "--event-stream",
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
            "--event-stream",
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
    let streamed = cli(
        dir.path(),
        &["run", "index.mq.md", "--event-stream", "jsonl"],
    );
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
fn trace_file_coexists_and_shares_the_run_identity() {
    let dir = TempDir::new("trace");
    dir.write("index.mq.md", BASIC);
    let trace_path = dir.path().join("trace.json");
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--event-stream",
            "jsonl",
            "--trace-file",
            trace_path.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let events = parse_events(&output.stdout);
    assert_envelope(&events);
    let trace: Value = serde_json::from_slice(&fs::read(&trace_path).unwrap()).unwrap();
    assert_eq!(trace["schema"], "marqdo.trace");
    assert_eq!(trace["schema_version"], 2);
    assert_eq!(trace["run"]["id"], events[0]["run_id"]);
    let started = events_of(&events, "run.started")[0];
    assert_eq!(
        started["payload"]["source"]["analysis_digest"],
        trace["source"]["analysis_digest"]
    );
    assert!(
        events_of(&events, "run.completed").len() == 1,
        "trace writing does not suppress the terminal event"
    );
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
            "--event-stream",
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
            "--event-stream",
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            "index.mq.md",
            "--event-stream",
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            "index.mq.md",
            "--event-stream",
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
