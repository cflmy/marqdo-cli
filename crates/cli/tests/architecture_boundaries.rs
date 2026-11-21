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

/// The CLI must consume the platform-neutral strict-child API. Direct use of
/// the Linux-only `marker_present`/`verify_active` symbols reintroduces the
/// macOS/Windows `E0425` build break (RFC-CI-0001 B01).
#[test]
fn cli_uses_only_the_platform_neutral_strict_child_api() {
    let root = repo_root();
    let run = read(&root.join("crates/cli/src/commands/run.rs"));
    assert!(
        run.contains("inspect_strict_child"),
        "run.rs must use marqdo_sandbox::inspect_strict_child"
    );
    for source in walk_rs(&root.join("crates/cli/src")) {
        let text = read(&source);
        for forbidden in ["marker_present", "verify_active"] {
            assert!(
                !text.contains(forbidden),
                "{} references Linux-only `{forbidden}`; use StrictChildState instead",
                source.display()
            );
        }
    }
    let sandbox = read(&root.join("crates/marqdo-sandbox/src/lib.rs"));
    assert!(
        sandbox.contains("pub fn inspect_strict_child"),
        "the sandbox crate must expose inspect_strict_child"
    );
    assert!(
        !sandbox.contains("pub use linux::{marker_present"),
        "the Linux-only marker functions must not be re-exported from the crate root"
    );
}

fn walk_rs(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read_dir {}: {error}", directory.display()));
        for entry in entries {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files
}

/// RFC-EXT-0001 §4/§14: the runtime must dispatch by the prepared binding and
/// never re-resolve a name or special-case a print function.
#[test]
fn callable_binding_is_the_only_static_resolution_path() {
    let root = repo_root();
    let loader = read(&root.join("crates/cli/src/loader.rs"));
    let interpreter = read(&root.join("crates/marqdo-runtime/src/interpreter.rs"));
    let run = read(&root.join("crates/cli/src/commands/run.rs"));
    let frontend = read(&root.join("crates/marqdo-frontend/src/calls.rs"));

    assert!(
        !loader.contains("program_uses_print"),
        "loader must not scan for per-function names (ADR-0058)"
    );
    assert!(
        !interpreter.contains("print_override"),
        "the runtime must not special-case print (ADR-0054/0058)"
    );
    assert!(
        !interpreter.contains("external_callables.is_empty"),
        "the runtime must not fall back to plugin guessing (ADR-0054)"
    );
    assert!(
        interpreter.contains("call_bindings.get"),
        "the runtime must dispatch by the prepared call binding table"
    );
    assert!(
        !interpreter.contains("builtins::find(&qualified)"),
        "a qualified name must not be re-resolved at run time"
    );
    assert!(
        !run.contains("external_callables")
            || !run.contains("external_callables")
            || !run.contains("push(marqdo_ir::callable::CallableDescriptor"),
        "run must not synthesise plugin descriptors (ADR-0054/§4.5)"
    );
    assert!(
        frontend.contains("bind_program_calls"),
        "the frontend must expose the single binding entry point"
    );
}

/// RFC-EXT-0001 §14.2/S4: no business-domain builtin or host-op branch may be
/// added for an ecosystem feature. These must be implemented as `.mq.md`,
/// plugin exports or provider streams.
#[test]
fn no_business_domain_builtins_or_host_ops() {
    let root = repo_root();
    let builtins = read(&root.join("crates/marqdo-ir/src/builtins.rs"));
    let host = read(&root.join("crates/marqdo-host/src/host.rs"));
    for forbidden in ["quantum.", "ai.chat", "vendor.", "demo.sensor", "marqdo."] {
        assert!(
            !builtins.contains(forbidden),
            "business builtin namespace `{forbidden}` must not exist"
        );
    }
    assert!(
        !host.contains("\"demo.sensor\""),
        "provider kinds must be data, not host-op string branches"
    );
}

/// RFC-EXT-0001 §8: official stdlib public aliases are verified metadata, not
/// hard-coded runtime names. Recompute the canonical metadata digest.
#[test]
fn official_stdlib_public_aliases_are_verified_metadata() {
    let root = repo_root().join("stdlib");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("stdlib.json")).unwrap()).unwrap();
    let aliases = manifest["public_aliases"]
        .as_array()
        .expect("stdlib.json must declare public_aliases");
    assert!(!aliases.is_empty());
    let mut hasher = Sha256::new();
    let mut entries: Vec<&serde_json::Value> = aliases.iter().collect();
    entries.sort_by_key(|alias| {
        (
            alias["target_module"].as_str().unwrap_or("").to_string(),
            alias["target_function"].as_str().unwrap_or("").to_string(),
        )
    });
    for alias in &entries {
        hasher.update(alias["target_module"].as_str().unwrap().as_bytes());
        hasher.update([0]);
        hasher.update(alias["target_function"].as_str().unwrap().as_bytes());
        hasher.update([0]);
        let mut names: Vec<&str> = alias["public_names"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        names.sort();
        for name in names {
            hasher.update(name.as_bytes());
            hasher.update([0]);
        }
    }
    let actual = format!("sha256:{}", hex(&hasher.finalize()));
    assert_eq!(
        manifest["aliases_digest"], actual,
        "stdlib.json aliases_digest must cover the public alias table"
    );
    for alias in aliases {
        let target_module = alias["target_module"].as_str().unwrap();
        let target_function = alias["target_function"].as_str().unwrap();
        let module = read(&root.join(target_module));
        assert!(
            module.contains(&format!("## {target_function}")),
            "public alias target `{target_module}::{target_function}` must exist"
        );
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
