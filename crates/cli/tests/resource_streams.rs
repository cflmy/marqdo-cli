//! End-to-end resource stream tests through the CLI.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
    Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run marqdo")
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

const READER: &str = r#"## main

**内存 = memory.new(1, 1)**

**流 = resource.open("file", "data.txt", ["read"])**

**第一次 = resource.read(流, 内存, 0, 2)**

**第二次 = resource.read(流, 内存, 2, 4)**

**第三次 = resource.read(流, 内存, 6, 1)**

**关闭 = resource.close(流)**

*[第一次.value.count, 第一次.value.eof, 第二次.value.count, 第二次.value.eof, 第三次.value.count, 第三次.value.eof, 内存[0], 内存[1], 内存[2], 内存[3], 内存[4], 内存[5], 关闭.ok]*
"#;

#[test]
fn project_file_stream_reads_chunks_into_memory() {
    let dir = TempDir::new("file-read");
    write_file(&dir.path().join("data.txt"), "abcdef");
    write_file(&dir.path().join("index.mq.md"), READER);
    let output = cli(
        dir.path(),
        &["run", "index.mq.md", "--allow-read", "data.txt", "--json"],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["success"], true);
    let values: Vec<&serde_json::Value> = report["value"]["items"]
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

const WRITER: &str = r#"## main

**内存 = memory.new(1, 1)**

**`内存[0] = 65`**

**`内存[1] = 66`**

**`内存[2] = 67`**

**写 = resource.open("file", "out.bin", ["write"])**

**写入 = resource.write(写, 内存, 0, 3)**

**关闭写 = resource.close(写)**

**读 = resource.open("file", "out.bin", ["read"])**

**读出 = resource.read(读, 内存, 0, 3)**

**关闭读 = resource.close(读)**

*[写入.value.count, 关闭写.ok, 读出.value.count, 读出.value.eof, 内存[0], 内存[1], 内存[2], 关闭读.ok]*
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
            "--json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["success"], true);
    let values = report["value"]["items"].as_array().expect("list result");
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

const COPY: &str = r#"## main

**输入 = resource.open("file", "data.txt", ["read"])**

**输出 = resource.open("file", "out.bin", ["write"])**

**内存 = memory.new(1, 1)**

**总数 = 0**

**继续 = true**

- `继续`

  **块 = resource.read(输入, 内存, 0, 16)**

  1. `块.ok`

     > resource.write(输出, 内存, 0, 块.value.count)

     **总数 = 总数 + 块.value.count**

     **继续 = not 块.value.eof**

  2. `true`

     **继续 = false**

**关闭读 = resource.close(输入)**

**关闭写 = resource.close(输出)**

*[总数, 关闭读.ok, 关闭写.ok]*
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
            "--json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["success"], true);
    let values = report["value"]["items"].as_array().expect("list result");
    assert_eq!(values[0]["decimal"], "6");
    assert_eq!(values[1]["value"], true);
    assert_eq!(values[2]["value"], true);
    assert_eq!(
        std::fs::read(dir.path().join("out.bin")).unwrap(),
        b"abcdef"
    );
}

#[test]
fn file_stream_requires_a_read_grant() {
    let dir = TempDir::new("file-denied");
    write_file(&dir.path().join("data.txt"), "abcdef");
    write_file(&dir.path().join("index.mq.md"), READER);
    let output = cli(dir.path(), &["run", "index.mq.md", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(report["success"], false);
    assert_eq!(report["error"]["code"], "permission.denied");
}
