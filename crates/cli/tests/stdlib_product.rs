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
    env: &'static [(&'static str, &'static str)],
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
                    env: &[],
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
                &[
                    "--allow-read",
                    "out",
                    "--allow-read",
                    "out/**",
                    "--allow-write",
                    "out",
                    "--allow-write",
                    "out/**",
                ][..]
            } else if name == "env.mq.md" {
                &[
                    "--allow-env",
                    "MARQDO_TEST_ENV",
                    "--allow-env",
                    "MARQDO_MISSING_ENV",
                ][..]
            } else {
                &[][..]
            };
            let env = if name == "env.mq.md" {
                &[("MARQDO_TEST_ENV", "env-value")][..]
            } else {
                &[][..]
            };
            TestFile {
                name,
                stdin,
                stdout,
                args,
                env,
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
        .envs(file.env.iter().copied())
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
            "clock.mq.md",
            "console.mq.md",
            "csv.mq.md",
            "datetime.mq.md",
            "demo.mq.md",
            "env.mq.md",
            "format.mq.md",
            "fs.mq.md",
            "http.mq.md",
            "io.mq.md",
            "json.mq.md",
            "path.mq.md",
            "regex.mq.md",
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
fn fs_test_exercises_the_file_operation_surface() {
    let _guard = lock_fs("fs.mq.md");
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "fs.mq.md")
            .expect("fs test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["decimal"], "7");
    assert_eq!(items[2]["value"], true);
    assert_eq!(items[3]["value"], "file");
    assert_eq!(items[4]["decimal"], "7");
    assert_eq!(items[5]["items"][0]["value"], "fs-a.txt");
    assert_eq!(items[6]["value"], true);
    assert_eq!(items[7]["value"], true);
    assert_eq!(items[8]["value"], true);
    assert_eq!(items[9]["value"], "你好\n");
    assert_eq!(items[10]["value"], true);
    assert_eq!(items[11]["value"], true);
    assert!(
        !stdlib_dir().join("tests/out").exists(),
        "the fs test did not remove its directory"
    );
}

#[test]
fn json_test_parses_and_formats_nested_values() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "json.mq.md")
            .expect("json test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["decimal"], "1");
    assert_eq!(items[2]["value"], "说\"好");
    assert_eq!(
        items[3]["value"],
        "[1, 2.5, -300.0, 0.4, {\"a\": 1.0}, \"x\"]"
    );
    assert_eq!(items[4]["value"], false);
    assert_eq!(items[5]["value"], "json.invalid_array");
    assert_eq!(items[6]["value"], "json.trailing");
}

#[test]
fn csv_test_parses_and_formats_quoted_rows() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "csv.mq.md")
            .expect("csv test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    let row = items[0]["items"].as_array().expect("row");
    assert_eq!(row.len(), 3);
    assert_eq!(row[0]["value"], "甲");
    assert_eq!(row[1]["value"], "乙,丙");
    assert_eq!(row[2]["value"], "说\"好");
    assert_eq!(items[1]["value"], "甲,\"乙,丙\",\"说\"\"好\"");
    let empty = items[2]["items"].as_array().expect("empty row");
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0]["value"], "");
    assert_eq!(items[3]["value"], "csv.unterminated_quote");
    let quoted = items[4]["items"].as_array().expect("quoted row");
    assert_eq!(quoted[0]["value"], "a");
    assert_eq!(quoted[1]["value"], "b");
}

#[test]
fn path_test_joins_and_normalizes_project_paths() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "path.mq.md")
            .expect("path test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], "a/b/c");
    assert_eq!(items[1]["value"], "c.txt");
    assert_eq!(items[2]["value"], "a/b");
    assert_eq!(items[3]["value"], "txt");
    assert_eq!(items[4]["value"], "a/c");
    assert_eq!(items[5]["value"], "/a/c");
    assert_eq!(items[6]["value"], "../a/b");
    assert_eq!(items[7]["value"], true);
    assert_eq!(items[8]["value"], false);
}

#[test]
fn regex_test_matches_anchors_classes_and_search() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "regex.mq.md")
            .expect("regex test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["value"], false);
    assert_eq!(items[2]["value"], true);
    assert_eq!(items[3]["value"], true);
    assert_eq!(items[4]["value"], true);
    assert_eq!(items[5]["value"], true);
    assert_eq!(items[6]["value"], true);
    assert_eq!(items[7]["value"], true);
    assert_eq!(items[8]["value"], true);
    assert_eq!(items[9]["value"], false);
    assert_eq!(items[10]["value"], "regex.invalid");
    assert_eq!(items[11]["value"], "regex.unsupported");
}

#[test]
fn datetime_test_formats_and_parses_utc_instants() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "datetime.mq.md")
            .expect("datetime test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], "1970-01-01T00:00:00Z");
    assert_eq!(items[1]["value"], "2023-11-14T22:13:20.123Z");
    assert_eq!(items[2]["decimal"], "0");
    assert_eq!(items[3]["decimal"], "1795248000123");
    assert_eq!(items[4]["value"], false);
    assert_eq!(items[5]["value"], false);
    assert_eq!(items[6]["value"], "1969-12-31T23:59:59.999Z");
}

#[test]
fn clock_test_reports_project_mode_unix_millis() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "clock.mq.md")
            .expect("clock test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["value"], "Int");
    assert_eq!(items[2]["value"], true);
}

#[test]
fn env_test_reads_only_granted_names() {
    let report = run_product_test(
        product_tests()
            .iter()
            .find(|file| file.name == "env.mq.md")
            .expect("env test"),
    );
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["value"], true);
    assert_eq!(items[1]["value"], true);
    assert_eq!(items[2]["value"], true);
    assert_eq!(items[3]["value"], "env-value");
    assert_eq!(items[4]["value"], false);
    assert_eq!(items[5]["value"], "env.not_found");
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
