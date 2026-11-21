//! Product-level tests for the whole bundled `marqdo-stdlib` tree.
//!
//! Every public module in `stdlib/README.md` has a `.mq.md` self-test under
//! `stdlib/tests/`, and this lane runs the whole directory through the real
//! user import path (`std:` + the verified bundle), not through a conformance
//! fixture or a Rust shortcut.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

static FS_LOCK: Mutex<()> = Mutex::new(());

fn lock_fs(name: &str) -> Option<MutexGuard<'static, ()>> {
    if name == "fs.mq.md" {
        Some(FS_LOCK.lock().unwrap_or_else(|error| error.into_inner()))
    } else {
        None
    }
}

fn stdlib_dir() -> PathBuf {
    repo_root().join("stdlib")
}

struct TestFile {
    name: String,
    stdin: &'static str,
    stdout: &'static str,
    args: &'static [&'static str],
}

fn product_tests() -> Vec<TestFile> {
    let dir = stdlib_dir().join("tests");
    let mut files: Vec<TestFile> = std::fs::read_dir(&dir)
        .expect("read stdlib tests")
        .map(|entry| {
            let path = entry.unwrap().path();
            if !path.is_file() {
                return TestFile {
                    name: String::new(),
                    stdin: "",
                    stdout: "",
                    args: &[],
                };
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let (stdin, stdout) = match name.as_str() {
                "console.mq.md" => ("hello\n", ""),
                "io.mq.md" => ("", "stdlib self-test\n"),
                "stream.mq.md" => ("", "stream\n"),
                _ => ("", ""),
            };
            let args = if name == "fs.mq.md" {
                &["--allow-read", "out/*", "--allow-write", "out/*"][..]
            } else {
                &[][..]
            };
            TestFile {
                name,
                stdin,
                stdout,
                args,
            }
        })
        .filter(|file| !file.name.is_empty())
        .collect();
    files.sort_by(|left, right| left.name.cmp(&right.name));
    files
}

fn run_product_test(file: &TestFile) -> serde_json::Value {
    if file.name == "fs.mq.md" {
        let _ = std::fs::remove_dir_all(stdlib_dir().join("tests/out"));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run"])
        .arg(stdlib_dir().join("tests").join(&file.name))
        .args(file.args)
        .args(["--json"])
        .current_dir(stdlib_dir())
        .env("MARQDO_STDLIB_DIR", stdlib_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn marqdo");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(file.stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().expect("wait marqdo");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{} failed: {}",
        file.name,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json report")
}

#[test]
fn every_public_stdlib_module_has_a_product_test() {
    let names: Vec<String> = product_tests().into_iter().map(|file| file.name).collect();
    assert_eq!(
        names,
        vec![
            "console.mq.md",
            "demo.mq.md",
            "format.mq.md",
            "fs.mq.md",
            "http.mq.md",
            "io.mq.md",
            "stream.mq.md",
            "table.mq.md",
            "text.mq.md",
            "utf8.mq.md",
        ],
        "a public stdlib module is missing a product test"
    );
}

#[test]
fn the_whole_bundled_library_runs_through_std_imports() {
    for file in product_tests() {
        let _guard = lock_fs(&file.name);
        let report = run_product_test(&file);
        assert_eq!(report["success"], true, "{}", file.name);
        assert_eq!(
            report["stdout"], file.stdout,
            "{} wrote unexpected stdout",
            file.name
        );
    }
}

#[test]
fn fs_test_reads_writes_and_copies_project_files() {
    let _guard = lock_fs("fs.mq.md");
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "fs.mq.md")
            .expect("fs test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["decimal"], "7");
    assert_eq!(items[1]["value"], "你好\n");
    assert_eq!(items[2]["decimal"], "7");
    assert_eq!(items[3]["value"], "你好\n");
    let out = stdlib_dir().join("tests/out");
    assert_eq!(
        std::fs::read(out.join("fs-a.txt")).unwrap(),
        "你好\n".as_bytes()
    );
    assert_eq!(
        std::fs::read(out.join("fs-b.txt")).unwrap(),
        "你好\n".as_bytes()
    );
}

#[test]
fn format_test_pins_value_semantics_and_handle_observation() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "format.mq.md")
            .expect("format test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["decimal"], "3");
    assert_eq!(items[1]["value"], "[\"文本\", 2, 1.5]");
    assert_eq!(items[2]["value"], "{a: 1}");
    assert_eq!(items[3]["value"], "[\"a\", 2]");
    assert_eq!(items[4]["value"], "None");
}

#[test]
fn text_and_utf8_tests_agree_on_the_byte_encoding() {
    let text = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "text.mq.md")
            .expect("text test"),
    );
    let utf8 = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "utf8.mq.md")
            .expect("utf8 test"),
    );
    assert_eq!(text["value"]["items"][0]["decimal"], "2");
    assert_eq!(text["value"]["items"][1]["decimal"], "195");
    assert_eq!(text["value"]["items"][2]["decimal"], "169");
    assert_eq!(utf8["value"]["items"][0]["decimal"], "2");
    assert_eq!(utf8["value"]["items"][1]["value"], "é");
    assert_eq!(utf8["value"]["items"][2]["value"], false);
}

#[test]
fn stream_and_console_use_the_resource_endpoints() {
    let stream = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "stream.mq.md")
            .expect("stream test"),
    );
    assert_eq!(stream["value"]["items"][0]["value"], true);
    assert_eq!(stream["value"]["items"][1]["decimal"], "7");

    let console = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "console.mq.md")
            .expect("console test"),
    );
    assert_eq!(console["value"]["items"][0]["value"], true);
    assert_eq!(console["value"]["items"][1]["value"], "hello");
    assert!(
        console["trace"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["op"] == "resource.read" || event["op"] == "resource.open"),
        "console test did not go through a resource endpoint"
    );
}
