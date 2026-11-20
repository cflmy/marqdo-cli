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

/// First byte offset at which two inputs differ, if any.
fn first_byte_difference(expected: &[u8], actual: &[u8]) -> Option<usize> {
    expected
        .iter()
        .zip(actual.iter())
        .position(|(left, right)| left != right)
        .or_else(|| (expected.len() != actual.len()).then_some(expected.len().min(actual.len())))
}

/// Render a short escaped byte window around `offset`.
fn byte_window(bytes: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(8);
    let end = (offset + 8).min(bytes.len());
    bytes[start..end]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether two byte strings are identical after normalizing CRLF to LF.
fn only_line_endings_differ(expected: &[u8], actual: &[u8]) -> bool {
    fn normalize(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                out.push(b'\n');
                index += 2;
            } else {
                out.push(bytes[index]);
                index += 1;
            }
        }
        out
    }
    expected != actual && normalize(expected) == normalize(actual)
}

/// Report a compact, byte-aware diff summary for one fixture.
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
         expected {} byte(s), actual {} byte(s).\n\
         {} line(s) differ (first at line {}).\n",
        fixture.display(),
        expected.len(),
        actual.len(),
        differences,
        first + 1
    );
    match first_byte_difference(expected.as_bytes(), actual.as_bytes()) {
        Some(offset) => {
            message.push_str(&format!(
                "first byte difference at offset {offset}.\n\
                 expected bytes: {}\n\
                 actual bytes:   {}\n",
                byte_window(expected.as_bytes(), offset),
                byte_window(actual.as_bytes(), offset)
            ));
            if only_line_endings_differ(expected.as_bytes(), actual.as_bytes()) {
                message.push_str(
                    "the only difference is line endings (CRLF vs LF); \
                     this is a checkout/fixture byte-identity problem, not a \
                     Markdown semantics change. Ensure .gitattributes keeps \
                     goldens at LF and keep intentional CRLF inputs as byte \
                     fixtures.\n",
                );
            }
        }
        None => message.push_str("the two strings are byte-identical.\n"),
    }
    message.push_str("--- expected (golden) ---\n");
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

/// Goldens are canonical JSON: UTF-8, LF line endings and a trailing newline.
/// A CRLF checkout used to fail with "0 line(s) differ", which hid the real
/// cause. This test fails early with a direct byte-level diagnosis instead.
#[test]
fn ast_goldens_use_canonical_lf_bytes() {
    let dir = ast_dir();
    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).expect("ast fixture directory is readable") {
        let path = entry.expect("directory entry").path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            continue;
        }
        let bytes = fs::read(&path).expect("golden is readable");
        assert!(
            std::str::from_utf8(&bytes).is_ok(),
            "golden {} is not valid UTF-8",
            path.display()
        );
        assert!(
            !bytes.contains(&b'\r'),
            "golden {} contains CR bytes; canonical JSON goldens must use LF only",
            path.display()
        );
        assert!(
            bytes.ends_with(b"\n"),
            "golden {} must end with exactly one trailing LF",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 100,
        "expected the full AST golden corpus, saw {checked}"
    );
}

/// The profile's line-ending semantics are tested with an explicit byte
/// fixture, not by relying on the checkout platform. Exactly one fixture keeps
/// CRLF bytes; see `.gitattributes`.
#[test]
fn crlf_fixture_is_preserved_as_bytes() {
    let fixture = ast_dir().join("146-crlf-line-endings.mq.md");
    let bytes = fs::read(&fixture).expect("CRLF fixture is readable");
    assert!(
        bytes.windows(2).any(|window| window == b"\r\n"),
        "the CRLF fixture lost its CRLF bytes (checkout normalization?)"
    );
    assert!(
        bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte != b'\r' || bytes.get(index + 1) == Some(&b'\n')),
        "CRLF fixture contains a lone CR byte"
    );
    assert!(
        !bytes
            .split(|byte| *byte == b'\n')
            .any(|line| !line.ends_with(b"\r") && !line.is_empty()),
        "CRLF fixture mixes bare LF line endings with CRLF"
    );

    let golden = ast_dir().join("146-crlf-line-endings.ast.json");
    let golden_bytes = fs::read(&golden).expect("CRLF golden is readable");
    assert!(
        !golden_bytes.contains(&b'\r'),
        "the CRLF fixture's golden is canonical LF JSON, not CRLF text"
    );

    let mut crlf_fixtures = 0usize;
    for entry in fs::read_dir(ast_dir()).expect("ast fixture directory is readable") {
        let path = entry.expect("directory entry").path();
        if path.extension().is_some_and(|extension| extension == "md") {
            let bytes = fs::read(&path).expect("fixture is readable");
            if bytes.contains(&b'\r') {
                assert_eq!(
                    path.file_name(),
                    fixture.file_name(),
                    "unexpected CRLF fixture {}; add it deliberately and document it",
                    path.display()
                );
                crlf_fixtures += 1;
            }
        }
    }
    assert_eq!(
        crlf_fixtures, 1,
        "exactly one CRLF byte fixture is expected"
    );
}

#[test]
fn byte_diff_helpers_distinguish_line_endings_from_semantics() {
    assert_eq!(first_byte_difference(b"abc", b"abc"), None);
    assert_eq!(first_byte_difference(b"abc", b"abd"), Some(2));
    assert_eq!(first_byte_difference(b"ab", b"abc"), Some(2));
    assert!(only_line_endings_differ(b"a\nb\n", b"a\r\nb\r\n"));
    assert!(!only_line_endings_differ(b"a\nb\n", b"a\nc\n"));
    assert!(!only_line_endings_differ(b"a\r\nb\n", b"a\r\nb\n"));
}
