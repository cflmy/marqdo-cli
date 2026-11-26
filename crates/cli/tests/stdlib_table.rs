//! CM-A08 table stdlib module tests.

#[path = "support/observation.rs"]
pub mod observation;

#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::process::Command;

fn run_table_self_test() -> serde_json::Value {
    let bundle = stdlib_support::bundle_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args(["run", "tests/table.mq.md", "--output", "json"])
        .current_dir(&bundle)
        .env("MARQDO_STDLIB_DIR", &bundle)
        .output()
        .expect("run table self-test");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json output")
}

fn rows(value: &serde_json::Value) -> &Vec<serde_json::Value> {
    value["rows"].as_array().expect("rows")
}

#[test]
fn stdlib_table_algorithms_rebuild_without_mutating_inputs() {
    let report = run_table_self_test();
    assert_eq!(report["run"]["status"], "ok");
    let items = observation::terminal(&report)["value"]["items"]
        .as_array()
        .expect("list");
    assert_eq!(items.len(), 9);

    assert_eq!(items[0]["header"], serde_json::json!(["分数"]));
    assert_eq!(items[0]["rows"].as_array().unwrap().len(), 3);

    assert_eq!(rows(&items[1]).len(), 2);
    assert_eq!(rows(&items[2]).len(), 3);
    assert_eq!(rows(&items[2])[0][1]["decimal"], "71");
    assert_eq!(rows(&items[3]).len(), 4);
    assert_eq!(rows(&items[3])[3][0]["value"], "丁");
    assert_eq!(rows(&items[4])[0][1]["decimal"], "99");

    assert_eq!(rows(&items[5]).len(), 2);
    assert_eq!(items[5]["header"].as_array().unwrap().len(), 4);

    assert_eq!(
        items[6]["entries"]["a"]["items"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        items[6]["entries"]["b"]["items"].as_array().unwrap().len(),
        1
    );

    // Original input is unchanged.
    assert_eq!(rows(&items[7]).len(), 3);
    assert_eq!(rows(&items[7])[0][1]["decimal"], "87");

    assert_eq!(
        items[8]["value"],
        "| 名称 | 分数 |\n| --- | --- |\n| 甲 | 87 |\n| 乙 | 92 |\n| 丙 | 71 |"
    );
}
