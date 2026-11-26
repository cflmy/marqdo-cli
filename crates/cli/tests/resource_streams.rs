//! End-to-end resource stream tests through the CLI.
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

#[path = "support/observation.rs"]
pub mod observation;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("marqdo-resource-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
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

fn cli(dir: &Path, args: &[&str]) -> Output {
    stdlib_support::cli_command()
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run marqdo")
}

fn cli_with_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = stdlib_support::cli_command()
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn marqdo");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(input)
        .expect("write stdin");
    child.wait_with_output().expect("wait for marqdo")
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

const READER: &str = r#"## @exec main

`内存 = memory.new(1, 1)`

`流 = resource.open("file", "data.txt", ["read"])`

`第一次 = resource.read(流, 内存, 0, 2)`

`第二次 = resource.read(流, 内存, 2, 4)`

`第三次 = resource.read(流, 内存, 6, 1)`

`关闭 = resource.close(流)`

`return [第一次.value.count, 第一次.value.eof, 第二次.value.count, 第二次.value.eof, 第三次.value.count, 第三次.value.eof, 内存[0], 内存[1], 内存[2], 内存[3], 内存[4], 内存[5], 关闭.ok]`
"#;

#[test]
fn project_file_stream_reads_chunks_into_memory() {
    let dir = TempDir::new("file-read");
    write_file(&dir.path().join("data.txt"), "abcdef");
    write_file(&dir.path().join("index.mq.md"), READER);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--allow-read",
            "data.txt",
            "--output",
            "json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    let values: Vec<&serde_json::Value> = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result")
        .iter()
        .collect();
    assert_eq!(values[0]["decimal"], "2");
    assert_eq!(values[1]["value"], false);
    assert_eq!(values[2]["decimal"], "4");
    assert_eq!(values[3]["value"], true);
    assert_eq!(values[4]["decimal"], "0");
    assert_eq!(values[5]["value"], true);
    for (index, expected) in ["97", "98", "99", "100", "101", "102"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(values[6 + index]["decimal"], expected);
    }
    assert_eq!(values[12]["value"], true);
}

const WRITER: &str = r#"## @exec main

`内存 = memory.new(1, 1)`

`内存[0] = 65`

`内存[1] = 66`

`内存[2] = 67`

`写 = resource.open("file", "out.bin", ["write"])`

`写入 = resource.write(写, 内存, 0, 3)`

`关闭写 = resource.close(写)`

`读 = resource.open("file", "out.bin", ["read"])`

`读出 = resource.read(读, 内存, 0, 3)`

`关闭读 = resource.close(读)`

`return [写入.value.count, 关闭写.ok, 读出.value.count, 读出.value.eof, 内存[0], 内存[1], 内存[2], 关闭读.ok]`
"#;

#[test]
fn project_file_stream_writes_on_close_then_reads_back() {
    let dir = TempDir::new("file-write");
    write_file(&dir.path().join("index.mq.md"), WRITER);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--allow-write",
            "out.bin",
            "--allow-read",
            "out.bin",
            "--output",
            "json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result");
    assert_eq!(values[0]["decimal"], "3");
    assert_eq!(values[1]["value"], true);
    assert_eq!(values[2]["decimal"], "3");
    assert_eq!(values[3]["value"], true);
    assert_eq!(values[4]["decimal"], "65");
    assert_eq!(values[5]["decimal"], "66");
    assert_eq!(values[6]["decimal"], "67");
    assert_eq!(values[7]["value"], true);
    assert_eq!(std::fs::read(dir.path().join("out.bin")).unwrap(), b"ABC");
}

const COPY: &str = r#"## @exec main

`输入 = resource.open("file", "data.txt", ["read"])`

`输出 = resource.open("file", "out.bin", ["write"])`

`内存 = memory.new(1, 1)`

`总数 = 0`

`继续 = true`

`while 继续`

`块 = resource.read(输入, 内存, 0, 16)`

`if 块.ok`

`resource.write(输出, 内存, 0, 块.value.count)`

`总数 = 总数 + 块.value.count`

`继续 = not 块.value.eof`

`else`

`继续 = false`

`end`

`end`

`关闭读 = resource.close(输入)`

`关闭写 = resource.close(输出)`

`return [总数, 关闭读.ok, 关闭写.ok]`
"#;

#[test]
fn file_copy_is_written_in_mqmd_through_chunked_streams() {
    let dir = TempDir::new("file-copy");
    write_file(&dir.path().join("data.txt"), "abcdef");
    write_file(&dir.path().join("index.mq.md"), COPY);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--allow-read",
            "data.txt",
            "--allow-write",
            "out.bin",
            "--output",
            "json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result");
    assert_eq!(values[0]["decimal"], "6");
    assert_eq!(values[1]["value"], true);
    assert_eq!(values[2]["value"], true);
    assert_eq!(
        std::fs::read(dir.path().join("out.bin")).unwrap(),
        b"abcdef"
    );
}

#[test]
fn tcp_stream_round_trips_through_mqmd_with_a_network_grant() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).unwrap();
        stream.write_all(&buffer).unwrap();
    });

    let dir = TempDir::new("tcp");
    let program = format!(
        r#"## @exec main

`内存 = memory.new(1, 1)`

`内存[0] = 112`

`内存[1] = 105`

`内存[2] = 110`

`内存[3] = 103`

`流 = resource.open("tcp", "{address}", ["read", "write"])`

`写入 = resource.write(流, 内存, 0, 4)`

`读出 = resource.read(流, 内存, 0, 4)`

`关闭 = resource.close(流)`

`return [写入.value.count, 读出.value.count, 内存[0], 内存[1], 内存[2], 内存[3], 关闭.ok]`
"#
    );
    write_file(&dir.path().join("index.mq.md"), &program);
    let output = cli(
        dir.path(),
        &[
            "run",
            "index.mq.md",
            "--allow-net",
            "127.0.0.1",
            "--output",
            "json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list");
    assert_eq!(values[0]["decimal"], "4");
    assert_eq!(values[1]["decimal"], "4");
    for (index, expected) in ["112", "105", "110", "103"].into_iter().enumerate() {
        assert_eq!(values[2 + index]["decimal"], expected);
    }
    assert_eq!(values[6]["value"], true);
    server.join().unwrap();
}

#[test]
fn file_stream_requires_a_read_grant() {
    let dir = TempDir::new("file-denied");
    write_file(&dir.path().join("data.txt"), "abcdef");
    write_file(&dir.path().join("index.mq.md"), READER);
    let output = cli(dir.path(), &["run", "index.mq.md", "--output", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_ne!(report["run"]["status"], "ok");
    assert_eq!(
        observation::terminal(&report)["error"]["code"],
        "permission.denied"
    );
}

const STDIN_READER: &str = r#"## @exec main

`输入 = resource.open("stdio", "stdin", ["read"])`

`内存 = memory.new(1, 1)`

`第一 = resource.read(输入, 内存, 0, 2)`

`第二 = resource.read(输入, 内存, 2, 8)`

`第三 = resource.read(输入, 内存, 0, 1)`

`return [第一.value.count, 第一.value.eof, 第二.value.count, 第二.value.eof, 第三.value.eof, 内存[0], 内存[1], 内存[2], 内存[3]]`
"#;

#[test]
fn piped_stdin_is_chunked_and_reaches_eof() {
    let dir = TempDir::new("stdin");
    write_file(&dir.path().join("index.mq.md"), STDIN_READER);
    let output = cli_with_stdin(
        dir.path(),
        &["run", "index.mq.md", "--output", "json"],
        b"abcd",
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result");
    assert_eq!(values[0]["decimal"], "2");
    assert_eq!(values[1]["value"], false);
    assert_eq!(values[2]["decimal"], "2");
    assert_eq!(values[3]["value"], true);
    assert_eq!(values[4]["value"], true);
    for (index, expected) in ["97", "98", "99", "100"].into_iter().enumerate() {
        assert_eq!(values[5 + index]["decimal"], expected);
    }
}

const STDERR_WRITER: &str = r#"## @exec main

`错误 = resource.open("stdio", "stderr", ["write"])`

`输出 = resource.open("stdio", "stdout", ["write"])`

`内存 = memory.new(1, 1)`

`内存[0] = 101`

`内存[1] = 114`

`内存[2] = 114`

`写错误 = resource.write(错误, 内存, 0, 3)`

`内存[0] = 111`

`内存[1] = 117`

`内存[2] = 116`

`写输出 = resource.write(输出, 内存, 0, 3)`

`关闭错误 = resource.close(错误)`

`关闭输出 = resource.close(输出)`

`return [写错误.value.count, 写输出.value.count, 关闭错误.ok, 关闭输出.ok]`
"#;

#[test]
fn program_stderr_is_separate_from_stdout_and_diagnostics() {
    let dir = TempDir::new("stderr");
    write_file(&dir.path().join("index.mq.md"), STDERR_WRITER);
    let output = cli(dir.path(), &["run", "index.mq.md", "--output", "json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    assert_eq!(observation::text(&report, "stdout.chunk"), "out");
    assert_eq!(observation::text(&report, "stderr.chunk"), "err");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result");
    assert_eq!(values[0]["decimal"], "3");
    assert_eq!(values[1]["decimal"], "3");
    assert_eq!(values[2]["value"], true);
    assert_eq!(values[3]["value"], true);
}

const CONSOLE_READER: &str = r#"---
import:
  console: "std:io/console"
---

## @exec main

`第一 = console.读取行()`

`第二 = console.读取行()`

`第三 = console.读取行()`

`输出 = console.打印(第一.value + "|" + 第二.value)`

`return [第一.ok, 第二.ok, 第三.ok, 第三.code, 输出.ok]`
"#;

#[test]
fn stdlib_console_reads_lines_and_reports_eof() {
    let dir = TempDir::new("console");
    write_file(&dir.path().join("index.mq.md"), CONSOLE_READER);
    let output = cli_with_stdin(
        dir.path(),
        &["run", "index.mq.md", "--output", "json"],
        b"one\ntwo\n",
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["run"]["status"], "ok");
    assert_eq!(observation::text(&report, "stdout.chunk"), "one|two\n");
    let values = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list result");
    assert_eq!(values[0]["value"], true);
    assert_eq!(values[1]["value"], true);
    assert_eq!(values[2]["value"], false);
    assert_eq!(values[3]["value"], "io.eof");
    assert_eq!(values[4]["value"], true);
}
