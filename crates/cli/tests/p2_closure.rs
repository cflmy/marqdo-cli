//! RFC-CM P2 closure lane: C01–C06/C11/C12 in a pure, no-Foreign/no-Plugin host.

#[path = "support/observation.rs"]
pub mod observation;

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture(name: &str) -> PathBuf {
    repo_root()
        .join("tests")
        .join("conformance")
        .join("semantics")
        .join(name)
}

fn expected(name: &str) -> serde_json::Value {
    let path = fixture(&name.replace(".mq.md", ".expected.json"));
    serde_json::from_slice(&std::fs::read(&path).expect("expected golden")).expect("golden json")
}

#[test]
fn p2_algorithms_run_in_a_pure_no_foreign_no_plugin_host() {
    // P2 gate (RFC-CM-0002 §8): C01-C06, C11, C12 use only .mq.md plus the
    // declared low-level primitives. These fixtures are run through the real
    // CLI in `--mode pure`, which has no foreign/plugin grants, and the source
    // is checked for host shortcuts that would violate the anti-evasion rule.
    let cases: &[(&str, &str)] = &[
        ("48-memory-bump-allocator.mq.md", "C01 bump allocator"),
        (
            "50-memory-allocation-failure-atomic.mq.md",
            "C01 allocation atomicity",
        ),
        (
            "55-first-fit-free-list-allocator.mq.md",
            "C01 free-list split/coalesce",
        ),
        ("49-memory-linked-list.mq.md", "C02 linked list"),
        ("56-memory-binary-tree.mq.md", "C02 tree"),
        ("61-memory-graph-adjacency.mq.md", "C02 graph"),
        ("58-memory-hash-table.mq.md", "C02 hash table"),
        ("64-memory-integer-codecs.mq.md", "C03 codecs"),
        ("59-utf8-encoder.mq.md", "C04 UTF-8 encode"),
        ("60-utf8-decoder.mq.md", "C04 UTF-8 decode"),
        ("57-csv-row-splitter.mq.md", "C05 CSV"),
        ("62-json-array-parser.mq.md", "C05 JSON"),
        ("68-csv-quoted-row.mq.md", "C05 CSV quoted"),
        ("53-table-projection.mq.md", "C06 table projection"),
        ("54-table-filter-stdlib.mq.md", "C06 table filter"),
        ("65-table-sort-stdlib.mq.md", "C06 table sort"),
        ("67-table-join-stdlib.mq.md", "C06 table join"),
        ("69-table-group-stdlib.mq.md", "C06 table group"),
        ("63-memory-bytecode-vm.mq.md", "C11 bytecode VM"),
        ("70-matrix-multiply.mq.md", "C12 matrix"),
        ("73-sparse-matrix.mq.md", "C12 sparse matrix"),
    ];
    for (name, criterion) in cases {
        let path = fixture(name);
        let source = std::fs::read_to_string(&path).expect("fixture source");
        for forbidden in ["foreign.", "plugin.", "import:"] {
            assert!(
                !source.contains(forbidden),
                "{criterion} fixture {name} uses forbidden host shortcut `{forbidden}`"
            );
        }
        let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
            .args([
                "run",
                path.to_str().unwrap(),
                "--mode",
                "pure",
                "--output",
                "json",
            ])
            .current_dir(repo_root())
            .output()
            .expect("run marqdo");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{criterion} fixture {name} failed in pure mode: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("json report");
        assert_eq!(report["run"]["status"], "ok", "{name}");
        let golden = expected(name);
        assert_eq!(
            observation::terminal(&report)["value"],
            golden["value"],
            "{criterion} fixture {name}"
        );
        assert_eq!(
            observation::text(&report, "stdout.chunk"),
            golden["stdout"].as_str().unwrap_or_default(),
            "{criterion} fixture {name} stdout"
        );
    }
}

#[test]
fn p2_stdlib_table_also_runs_in_pure_mode() {
    let path = repo_root().join("stdlib").join("tests").join("table.mq.md");
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            path.to_str().unwrap(),
            "--mode",
            "pure",
            "--output",
            "json",
        ])
        .current_dir(repo_root().join("stdlib"))
        .env("MARQDO_STDLIB_DIR", repo_root().join("stdlib"))
        .output()
        .expect("run table self-test");
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
        .expect("table test list");
    assert_eq!(items.len(), 9);
    assert_eq!(
        items[8]["value"],
        "| 名称 | 分数 |\n| --- | --- |\n| 甲 | 87 |\n| 乙 | 92 |\n| 丙 | 71 |"
    );
}

#[test]
fn foreign_and_plugin_shortcuts_are_unavailable_in_the_pure_lane() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = dir.path().join("foreign.mq.md");
    std::fs::write(
        &foreign,
        "## @knowledge 代码\n\n```python\nprint(1)\n```\n\n## @exec main\n\n`return foreign.call(ref(\"#代码\"), None)`\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            foreign.to_str().unwrap(),
            "--mode",
            "pure",
            "--output",
            "json",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run foreign fixture");
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_ne!(report["run"]["status"], "ok");
    assert_eq!(
        observation::terminal(&report)["error"]["code"],
        "permission.denied"
    );

    let plugin = dir.path().join("plugin.mq.md");
    std::fs::write(
        &plugin,
        "## @exec main\n\n`plugin.invoke(\"missing.export\", {})`\n\n`return None`\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_marqdo"))
        .args([
            "run",
            plugin.to_str().unwrap(),
            "--mode",
            "pure",
            "--output",
            "json",
        ])
        .current_dir(dir.path())
        .output()
        .expect("run plugin fixture");
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_ne!(report["run"]["status"], "ok");
}
