//! Architecture boundary gate (review RFC-CM P0/P1).
//!
//! These checks pin the documented layering: the runtime depends on the host
//! platform layer, the bundled stdlib stays in this repository, `std:` import
//! targets resolve only inside the bundle, and the host keeps the documented
//! stdio/file/TCP resource endpoints. They are source and digest checks, not an
//! OS-sandbox proof.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

const WORKSPACE_CRATES: &[&str] = &[
    "marqdo-span",
    "marqdo-value",
    "marqdo-expression",
    "marqdo-markdown",
    "marqdo-ir",
    "marqdo-frontend",
    "marqdo-math",
    "marqdo-foreign",
    "marqdo-plugin-abi",
    "marqdo-provider-worker",
    "marqdo-plugin-sdk",
    "marqdo-host",
    "marqdo-runtime",
    "marqdo-package",
    "marqdo-sandbox",
    "cli",
];

#[test]
fn workspace_crates_match_the_documented_layers() {
    let manifest = read(&repo_root().join("Cargo.toml"));
    let members = manifest
        .split("members = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("workspace members");
    let names: Vec<&str> = members
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .collect();
    let expected: Vec<String> = WORKSPACE_CRATES
        .iter()
        .map(|name| format!("crates/{name}"))
        .collect();
    assert_eq!(
        names,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "workspace members drifted; update docs/architecture/CURRENT.md with the layer"
    );
    assert!(
        !names.contains(&"crates/marqdo-stdlib"),
        "the stdlib must stay in this repository (ADR-0039), not become a crate"
    );
}

#[test]
fn runtime_depends_on_the_host_platform_layer_without_a_cycle() {
    let runtime = read(&repo_root().join("crates/marqdo-runtime/Cargo.toml"));
    assert!(
        runtime
            .lines()
            .any(|line| line.trim() == "marqdo-host = { workspace = true }"),
        "runtime must keep the documented marqdo-runtime -> marqdo-host direction"
    );
    let host = read(&repo_root().join("crates/marqdo-host/Cargo.toml"));
    assert!(
        !host.contains("marqdo-runtime"),
        "marqdo-host must not depend back on marqdo-runtime"
    );
}

#[test]
fn bundled_stdlib_stays_in_this_repository_and_matches_its_digest() {
    let root = repo_root().join("stdlib");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("stdlib.json")).unwrap()).unwrap();
    assert_eq!(manifest["name"], "marqdo-stdlib");
    assert_eq!(manifest["version"], "0.1.0");
    let declared = manifest["digest"].as_str().expect("digest string");
    let actual = format!("sha256:{}", tree_digest(&root).unwrap());
    assert_eq!(
        actual, declared,
        "stdlib digest mismatch; regenerate it with tools/pack-stdlib.py and commit the digest"
    );
}

#[test]
fn stdlib_imports_resolve_inside_the_bundle() {
    let root = repo_root().join("stdlib");
    for entry in walk_markdown(&root) {
        let is_test = entry.starts_with(root.join("tests"));
        let text = read(&entry);
        for target in import_targets(&text) {
            if let Some(path) = target.strip_prefix("std:") {
                assert!(
                    is_test,
                    "{} imports `{}`; a product stdlib module uses a relative bundle path",
                    entry.display(),
                    target
                );
                let resolved = root.join(path).with_extension("mq.md");
                assert!(
                    resolved.exists(),
                    "{} imports `{}`, which does not resolve inside the bundle",
                    entry.display(),
                    target
                );
                continue;
            }
            let resolved = entry.parent().unwrap().join(&target);
            assert!(
                resolved.starts_with(&root),
                "{} imports `{}` outside the bundle",
                entry.display(),
                target
            );
            assert!(
                resolved.exists(),
                "{} imports missing `{}`",
                entry.display(),
                target
            );
        }
    }
}

#[test]
fn host_keeps_the_documented_resource_endpoints() {
    let resource = read(&repo_root().join("crates/marqdo-host/src/resource.rs"));
    for endpoint in ["stdin", "stdout", "stderr", "file", "tcp"] {
        let literal = format!("\"{endpoint}\"");
        assert!(
            resource.contains(&literal),
            "host resource layer no longer mentions the documented `{endpoint}` endpoint"
        );
    }
}

fn import_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut in_import = false;
    for line in text.lines() {
        if line.trim() == "import:" {
            in_import = true;
            continue;
        }
        if in_import {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                in_import = false;
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_import = false;
                continue;
            }
            if let Some((_, value)) = trimmed.split_once(':') {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    targets.push(value.to_string());
                }
            }
        }
    }
    targets
}

fn walk_markdown(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk_markdown(&path));
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".mq.md"))
        {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn tree_digest(root: &Path) -> Result<String, String> {
    let mut files: Vec<(String, PathBuf)> = walk_markdown(root)
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            Ok((relative, path))
        })
        .collect::<Result<_, String>>()?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update(b"\0");
        hasher.update(std::fs::read(&path).map_err(|error| error.to_string())?);
        hasher.update(b"\0");
    }
    Ok(hex::encode(hasher.finalize()))
}
