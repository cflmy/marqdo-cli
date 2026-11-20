//! CM-A08/P5 stdlib HTTP framing module tests.

use std::path::Path;
use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn stdlib_http_parses_chunked_responses_and_builds_requests() {
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run", "tests/http.mq.md", "--json"])
        .current_dir(repo_root().join("stdlib"))
        .env("MARQDO_STDLIB_DIR", repo_root().join("stdlib"))
        .output()
        .expect("run http self-test");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["success"], true);
    let items = report["value"]["items"].as_array().expect("list");
    assert_eq!(items[0]["decimal"], "200");
    assert_eq!(items[1]["value"], "OK");
    assert_eq!(items[2]["value"], "text/plain");
    assert_eq!(items[3]["value"], "hello world");
    assert_eq!(
        items[4]["value"],
        "POST /echo HTTP/1.1\r\nHost: example.test\r\nContent-Length: 2\r\n\r\nok"
    );
}
