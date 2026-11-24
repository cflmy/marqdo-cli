//! Documentation consistency gate (review RFC-CM P0).
//!
//! The current architecture lives in one file, the status file must link it,
//! relative links must resolve, and a removed "running status" claim must not
//! come back. This is a source check, not a release proof.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

#[test]
fn current_architecture_has_one_overview_and_four_state_dimensions() {
    let current = read(&repo_root().join("docs/architecture/CURRENT.md"));
    for heading in [
        "## 1. Four state dimensions",
        "## 2. Responsibility layers",
        "## 3. Dependency direction and public contracts",
        "## 4. Extension path classification",
        "## 5. Gap and closure backlog",
        "## 6. Documentation governance",
    ] {
        assert!(current.contains(heading), "CURRENT.md is missing {heading}");
    }
    for dimension in [
        "Implemented",
        "Tested locally",
        "Bundled stdlib released",
        "Multi-platform verified",
    ] {
        assert!(
            current.contains(dimension),
            "CURRENT.md is missing the `{dimension}` state dimension"
        );
    }
    let baseline = current
        .lines()
        .find(|line| line.starts_with("- Baseline:"))
        .expect("CURRENT.md has no Baseline line");
    let sha = baseline
        .split('`')
        .nth(1)
        .expect("Baseline line has no backtick-quoted SHA");
    assert_eq!(sha.len(), 40, "Baseline SHA is not 40 hex digits: {sha}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "Baseline SHA is not hex: {sha}"
    );
}

#[test]
fn index_and_status_point_at_the_current_overview_and_history() {
    let root = repo_root();
    let index = read(&root.join("docs/architecture/README.md"));
    for target in ["CURRENT.md", "../STATUS.md", "../history/README.md"] {
        assert!(
            index.contains(target),
            "architecture index does not link {target}"
        );
    }
    // The index is an index, not a second running-status page.
    assert!(
        !index.contains("P5 providers") && !index.contains("P5 Provider"),
        "architecture index must not duplicate the current P5 status"
    );

    let status = read(&root.join("docs/STATUS.md"));
    assert!(
        status.contains("architecture/CURRENT.md"),
        "STATUS.md does not link the current overview"
    );
    assert!(
        !status.contains("is still `c9f01aa8"),
        "STATUS.md still claims the stale `c9f01aa8` remote baseline"
    );
}

#[test]
fn current_relative_markdown_links_resolve() {
    let root = repo_root();
    for relative in [
        "docs/architecture/CURRENT.md",
        "docs/architecture/README.md",
        "docs/STATUS.md",
        "docs/history/README.md",
    ] {
        let path = root.join(relative);
        let text = read(&path);
        let mut checked = BTreeSet::new();
        for target in markdown_targets(&text) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
            {
                continue;
            }
            let file = target.split('#').next().unwrap();
            if file.is_empty() || !checked.insert(file.to_string()) {
                continue;
            }
            let resolved = path.parent().unwrap().join(file);
            assert!(
                resolved.exists(),
                "{} links missing `{target}` (resolved {})",
                path.display(),
                resolved.display()
            );
        }
    }
}

/// CI/security status drift is a defect: the current architecture table must
/// not simultaneously claim a network endpoint is implemented and absent, and
/// the CI doc must not claim the GitHub-hosted Windows/macOS matrix does not
/// exist (it does; Woodpecker has no such agent).
#[test]
fn ci_and_architecture_status_do_not_contradict_the_workflows() {
    let root = repo_root();
    let current = read(&root.join("docs/architecture/CURRENT.md"));
    assert!(
        !current.contains("no DNS/TLS/server socket/UDP"),
        "CURRENT.md repeats the stale TCP row that contradicts the network endpoint row"
    );
    assert!(
        !current.contains("file metadata / directory / rename / remove / mkdir"),
        "CURRENT.md repeats the stale not-implemented filesystem row"
    );

    let ci = read(&root.join("docs/ci.md"));
    assert!(
        !ci.contains("no runners are configured"),
        "docs/ci.md denies the existing GitHub-hosted Windows/macOS matrix"
    );
    assert!(
        ci.contains("windows-latest") && ci.contains("macos-latest"),
        "docs/ci.md must name the native matrix it cannot replace with Woodpecker"
    );

    let historical = read(&root.join("docs/NEXT_RELEASE_CI_CONVERGENCE.md"));
    assert!(
        historical.contains("Historical / Superseded baseline"),
        "the old convergence plan must keep its historical banner"
    );
}

#[test]
fn evidence_files_are_sha_pinned() {
    let evidence = repo_root().join("docs/evidence");
    for entry in std::fs::read_dir(&evidence).expect("read evidence dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let text = read(&path);
        assert!(
            has_hex_sha(&text),
            "{} has no 7–40 hex commit/security SHA",
            path.display()
        );
    }
}

fn markdown_targets(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b')' && bytes[end] != b'\n' {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b')' {
                out.push(text[start..end].to_string());
                i = end;
            }
        }
        i += 1;
    }
    out
}

fn has_hex_sha(text: &str) -> bool {
    for word in text.split(|c: char| !c.is_ascii_hexdigit()) {
        if (7..=40).contains(&word.len()) {
            return true;
        }
    }
    false
}

/// CI is a consumer of the public protocols: its configuration must exercise
/// only the current contracts. A retired name in a pipeline is a latent
/// failure and implies a compatibility path was kept somewhere.
#[test]
fn ci_configuration_uses_only_current_protocols() {
    let root = repo_root();
    for relative in [
        ".woodpecker/main.yml",
        ".github/workflows/ci.yml",
        "docs/ci.md",
    ] {
        let text = read(&root.join(relative));
        for retired in [
            "--event-stream",
            "--trace-file",
            "execution-events",
            "execution_events",
            "analysis_v3_consumer",
            "trace_consumer",
            "analysis-v3",
        ] {
            assert!(
                !text.contains(retired),
                "{relative} still references retired `{retired}`"
            );
        }
    }
}

/// The current contract documents must describe the interface that exists.
/// A retired option or a path to a deleted spec in `spec/` or the current
/// developer guides is a release-record defect: consumers would implement a
/// contract the binary does not accept. Historical ADRs, evidence and
/// `docs/history/` keep their original text and are intentionally excluded.
#[test]
fn current_contract_documents_do_not_reference_retired_interfaces() {
    let root = repo_root();
    let mut files = vec![
        "COMPATIBILITY.md".to_string(),
        "README.md".to_string(),
        "docs/ci.md".to_string(),
        "docs/architecture/CURRENT.md".to_string(),
    ];
    for directory in ["spec", "docs/developers", "examples"] {
        let base = root.join(directory);
        let mut stack = vec![base];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("read contract docs") {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                    files.push(
                        path.strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
    }
    let retired = [
        "--trace-file",
        "--event-stream",
        "EffectTrace",
        "crates/marqdo-host/src/trace.rs",
        "spec/trace-v1.md",
        "spec/trace-v2.md",
        "spec/execution-events-v1.md",
        "spec/analysis-v1.md",
        "spec/analysis-v2.md",
        "spec/tool-v1.md",
        "worker_frame_invalid for an over-limit",
    ];
    for relative in files {
        let text = read(&root.join(&relative));
        for (number, line) in text.lines().enumerate() {
            for token in retired {
                if !line.contains(token) {
                    continue;
                }
                // A sentence that explicitly marks the interface as retired or
                // removed is documentation of the migration, not a claim that
                // it is available.
                let explained = line.contains("retired")
                    || line.contains("removed")
                    || line.contains("no longer");
                assert!(
                    explained,
                    "{relative}:{} still documents retired `{token}`",
                    number + 1
                );
            }
        }
    }
}
