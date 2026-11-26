//! CM-A08/P5 stdlib HTTP framing module tests.

#[path = "support/observation.rs"]
pub mod observation;

#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::process::Command;

#[test]
fn stdlib_http_parses_chunked_responses_and_builds_requests() {
    let bundle = stdlib_support::bundle_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run", "tests/http.mq.md", "--output", "json"])
        .current_dir(&bundle)
        .env("MARQDO_STDLIB_DIR", &bundle)
        .output()
        .expect("run http self-test");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(report["run"]["status"], "ok");
    let items = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list");
    assert_eq!(items[0]["decimal"], "200");
    assert_eq!(items[1]["value"], "OK");
    assert_eq!(items[2]["value"], "text/plain");
    assert_eq!(items[3]["value"], "hello world");
    assert_eq!(
        items[4]["value"],
        "POST /echo HTTP/1.1\r\nHost: example.test\r\nContent-Length: 2\r\n\r\nok"
    );
    assert_eq!(items[5]["decimal"], "200");
    assert_eq!(items[6]["value"], "你好");
    assert_eq!(items[7]["value"], "POST / HTTP/1.1\r\n\r\nAB");
}
