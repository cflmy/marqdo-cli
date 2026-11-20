//! AST conformance golden tests for the frozen `marqdo.md.v1` Markdown profile.
//!
//! Each `tests/conformance/ast/*.mq.md` fixture is parsed with
//! [`marqdo_markdown::parse`] and the canonical AST root is serialized to
//! pretty JSON. The result is compared byte-for-byte with the sibling
//! `*.ast.json` golden.
//!
//! Regenerate goldens deliberately with:
//!
//! ```text
//! UPDATE_GOLDENS=1 cargo test -p marqdo-cli --test conformance_ast
//! ```
//!
//! Updating is the *only* way goldens change; the test fails if a fixture has
//! no golden and if a golden has no fixture.

use std::fs;
use std::path::{Path, PathBuf};

/// Repository root (the workspace above `crates/cli`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/cli has a parent")
        .parent()
        .expect("crates has a parent")
        .to_path_buf()
}

fn ast_dir() -> PathBuf {
    repo_root().join("tests").join("conformance").join("ast")
}

/// All fixture paths, sorted by file name for deterministic ordering.
fn fixture_paths() -> Vec<PathBuf> {
    let dir = ast_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths
}

/// Serialize the canonical AST view with a trailing newline.
fn golden_text(source: &str) -> String {
    let document = marqdo_markdown::parse(source);
    let mut text = serde_json::to_string_pretty(&document.root)
        .expect("the canonical AST view is always serializable");
    text.push('\n');
    text
}

/// Golden path for a fixture: `foo.mq.md` -> `foo.ast.json` (the extension
/// replacement must keep the `.mq` marker, not overwrite `md` only).
fn golden_path(fixture: &Path) -> PathBuf {
    let name = fixture
        .file_name()
        .expect("fixture has a file name")
        .to_string_lossy();
    let stem = name
        .strip_suffix(".mq.md")
        .unwrap_or_else(|| panic!("fixture `{}` must end in .mq.md", fixture.display()));
    fixture.with_file_name(format!("{stem}.ast.json"))
}

fn write_golden(path: &Path, text: &str) {
    fs::write(path, text)
        .unwrap_or_else(|error| panic!("cannot write golden {}: {error}", path.display()));
}

/// Report a compact, line-based diff summary for one fixture.
fn report_mismatch(fixture: &Path, expected: &str, actual: &str) {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut differences = 0usize;
    let mut first: Option<usize> = None;
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        if expected_lines.get(index) != actual_lines.get(index) {
            differences += 1;
            if first.is_none() {
                first = Some(index);
            }
        }
    }
    let first = first.unwrap_or(0);
    let start = first.saturating_sub(3);
    let end = (first + 8).min(expected_lines.len().max(actual_lines.len()));
    let mut message = format!(
        "AST golden mismatch for `{}`.\n\
         {} line(s) differ (first at line {}).\n\
         --- expected (golden) ---\n",
        fixture.display(),
        differences,
        first + 1
    );
    for index in start..end {
        message.push_str(&format!(
            "{:>4} | {}\n",
            index + 1,
            expected_lines.get(index).copied().unwrap_or("<missing>")
        ));
    }
    message.push_str("--- actual (parser) ---\n");
    for index in start..end {
        message.push_str(&format!(
            "{:>4} | {}\n",
            index + 1,
            actual_lines.get(index).copied().unwrap_or("<missing>")
        ));
    }

    // The most common cause is an executable payload of the wrong shape. Point
    // at the offending source region so authors can fix the fixture quickly.
    if let Ok(source) = fs::read_to_string(fixture) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(expected) {
            if let Some(span) = value.get("span") {
                let start = span.get("start").and_then(serde_json::Value::as_u64);
                let end = span.get("end").and_then(serde_json::Value::as_u64);
                if let (Some(start), Some(end)) = (start, end) {
                    let slice = source.get(start as usize..end as usize).unwrap_or("");
                    message.push_str(&format!("first node source: {slice:?}\n"));
                }
            }
        }
    }

    message.push_str(
        "Run `UPDATE_GOLDENS=1 cargo test -p marqdo-cli --test conformance_ast` \
         only for an intentional profile change.\n",
    );
    panic!("{message}");
}

#[test]
fn ast_goldens_match_the_frozen_profile() {
    let fixtures = fixture_paths();
    assert!(
        fixtures.len() >= 100,
        "the P0 corpus must keep at least 100 AST fixtures, found {}",
        fixtures.len()
    );

    let update = std::env::var_os("UPDATE_GOLDENS").is_some_and(|value| value == "1");
    let mut updated = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for fixture in &fixtures {
        let source = fs::read_to_string(fixture)
            .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", fixture.display()));
        let golden = golden_path(fixture);
        let actual = golden_text(&source);

        if update {
            write_golden(&golden, &actual);
            updated += 1;
            continue;
        }

        let expected = fs::read_to_string(&golden).unwrap_or_else(|error| {
            panic!(
                "missing AST golden for `{}` ({error}).\n\
                 Run `UPDATE_GOLDENS=1 cargo test -p marqdo-cli --test conformance_ast`.",
                fixture.display()
            )
        });
        if expected != actual {
            failures.push(fixture.display().to_string());
        }
    }

    if update {
        println!(
            "UPDATE_GOLDENS=1: rewrote {updated} AST golden(s) under {}",
            ast_dir().display()
        );
        return;
    }

    // A golden without a fixture is stale corpus output; fail rather than keep
    // silently ignored files.
    let golden_count = fs::read_dir(ast_dir())
        .expect("ast fixture directory is readable")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .count();
    assert_eq!(
        golden_count,
        fixtures.len(),
        "every AST fixture needs exactly one golden (orphan *.ast.json files?)"
    );

    if failures.is_empty() {
        return;
    }

    // Re-read the mismatching fixtures to produce a detailed, line-based diff.
    // This keeps the common pass path cheap and the failure path informative.
    for fixture in &fixtures {
        let source = fs::read_to_string(fixture).expect("fixture is readable");
        let golden = golden_path(fixture);
        let expected = fs::read_to_string(&golden).expect("golden is readable");
        let actual = golden_text(&source);
        if expected != actual {
            report_mismatch(fixture, &expected, &actual);
        }
    }
}
