//! Architecture boundary gate (review RFC-CM P0/P1).
//!
//! These checks pin the documented layering: the engine depends on the neutral
//! Host contract (never the native host), the plugin ABI contract carries no
//! loader/worker execution side, the bundled stdlib stays in this repository,
//! `std:` import targets resolve only inside the bundle, and the host keeps the
//! documented stdio/file/TCP resource endpoints. They are source and digest
//! checks, not an OS-sandbox proof; `tools/ci/check_dependency_boundaries.py`
//! adds the Cargo-metadata graph gate.

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

/// Issue #13 / ADR-0071: the reviewed responsibility allowlist is the crate
/// table in the single current architecture overview, not a frozen count. A new
/// crate must be added there (and to the dependency gate's reviewed map) in the
/// same change.
#[test]
fn workspace_crates_are_documented_responsibilities() {
    let root = repo_root();
    let manifest = read(&root.join("Cargo.toml"));
    let members = manifest
        .split("members = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("workspace members");
    let names: Vec<String> = members
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .map(|member| {
            let directory = member
                .strip_prefix("crates/")
                .expect("workspace members are crates/<name>");
            let manifest = read(&root.join(member).join("Cargo.toml"));
            manifest
                .lines()
                .find_map(|line| line.strip_prefix("name = "))
                .map(|name| name.trim().trim_matches('"').to_string())
                .unwrap_or_else(|| directory.to_string())
        })
        .collect();
    let current = read(&root.join("docs/architecture/CURRENT.md"));
    let responsibilities = current
        .split("## Workspace crate responsibility map")
        .nth(1)
        .expect("CURRENT.md must own the reviewed crate responsibility map")
        .split("\n## ")
        .next()
        .unwrap_or("");
    for name in &names {
        assert!(
            responsibilities.contains(&format!("`{name}`")),
            "workspace crate `{name}` has no documented responsibility in CURRENT.md"
        );
    }
    for name in [
        "marqdo-host-contract",
        "marqdo-host",
        "marqdo-plugin-abi",
        "marqdo-cli",
    ] {
        assert!(
            names.iter().any(|crate_name| crate_name == name),
            "required layer crate `{name}` is missing from the workspace"
        );
    }
    assert!(
        !names.iter().any(|name| name == "marqdo-stdlib"),
        "the stdlib is a Bundle input, not a workspace crate (ADR-0039/ADR-0070)"
    );
}

#[test]
fn engine_depends_on_the_neutral_host_contract_only() {
    let root = repo_root();
    let runtime = read(&root.join("crates/marqdo-runtime/Cargo.toml"));
    let runtime_dependencies = runtime
        .split("[dependencies]")
        .nth(1)
        .expect("runtime dependencies")
        .split("[dev-dependencies]")
        .next()
        .expect("runtime normal dependencies");
    assert!(
        runtime_dependencies
            .lines()
            .any(|line| line.trim() == "marqdo-host-contract = { workspace = true }"),
        "runtime must depend on the neutral marqdo-host-contract crate"
    );
    assert!(
        !runtime_dependencies
            .lines()
            .any(|line| line.trim() == "marqdo-host = { workspace = true }"),
        "runtime normal/build dependencies must not link the native host"
    );
    let contract = read(&root.join("crates/marqdo-host-contract/Cargo.toml"));
    let contract_deps: Vec<&str> = contract
        .split("[dependencies]")
        .nth(1)
        .expect("contract dependencies")
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with('['))
        .collect();
    assert_eq!(
        contract_deps,
        ["marqdo-span", "marqdo-value"],
        "the host contract must stay platform neutral (value/span only)"
    );
    let host = read(&root.join("crates/marqdo-host/Cargo.toml"));
    for forbidden in ["marqdo-runtime", "marqdo-frontend", "marqdo-ir ="] {
        assert!(
            !host.contains(forbidden),
            "the native host must not depend back on `{forbidden}`"
        );
    }
    assert!(
        host.contains("marqdo-host-contract = { workspace = true }"),
        "the native host must implement the neutral contract crate"
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
fn shared_worker_transport_is_neutral_and_not_duplicated() {
    let root = repo_root();
    let transport = read(&root.join("crates/marqdo-worker-ipc/Cargo.toml"));
    for forbidden in ["marqdo-host", "marqdo-plugin-abi", "marqdo-provider-worker"] {
        assert!(
            !transport.contains(forbidden),
            "marqdo-worker-ipc must stay transport-only; it must not depend on `{forbidden}`"
        );
    }
    for user in [
        "crates/marqdo-host/Cargo.toml",
        "crates/marqdo-provider-worker/Cargo.toml",
    ] {
        assert!(
            read(&root.join(user)).contains("marqdo-worker-ipc = {"),
            "{user} must use the one shared worker transport"
        );
    }
    // The four-byte length prefix and its bounds live in exactly one crate.
    for private in [
        "crates/marqdo-host/src/plugin_worker.rs",
        "crates/marqdo-host/src/plugin/worker.rs",
        "crates/marqdo-host/src/provider_worker.rs",
        "crates/marqdo-provider-worker/src/main.rs",
    ] {
        let text = read(&root.join(private));
        let implementation = text.split("#[cfg(test)]").next().unwrap_or(&text);
        assert!(
            !implementation.contains("from_be_bytes") && !implementation.contains("to_be_bytes"),
            "{private} reimplements worker framing instead of using marqdo-worker-ipc"
        );
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

/// ADR-0064 freezes the core catalogue until a new language primitive is reviewed.
#[test]
fn builtin_names_and_aliases_are_reviewed() {
    let builtins = marqdo_ir::builtins::BUILTINS;
    let mut names: Vec<_> = builtins
        .iter()
        .flat_map(|builtin| std::iter::once(builtin.name).chain(builtin.aliases.iter().copied()))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "float",
            "foreign.call",
            "foreign.run",
            "fs.read",
            "fs.write",
            "int",
            "len",
            "list.append",
            "list.concat",
            "map.get",
            "map.has",
            "map.keys",
            "map.remove",
            "map.set",
            "memory.grow",
            "memory.new",
            "memory.size",
            "print",
            "ref",
            "resource.accept",
            "resource.close",
            "resource.open",
            "resource.read",
            "resource.request",
            "resource.write",
            "str",
            "table",
            "table.cell",
            "table.columns",
            "table.row",
            "text.codepoints",
            "text.from_codepoint",
            "type",
            "小数",
            "打印",
            "整数",
            "文本",
            "类型",
            "表",
            "长度",
        ],
        "review changes under ADR-0064"
    );
}

// Exhaustive matches make a new HostOp or Value variant require review under ADR-0064.
const _: fn(marqdo_host_contract::HostOp) = |operation| match operation {
    marqdo_host_contract::HostOp::ForeignRun
    | marqdo_host_contract::HostOp::ForeignCall
    | marqdo_host_contract::HostOp::FsRead
    | marqdo_host_contract::HostOp::FsWrite
    | marqdo_host_contract::HostOp::PluginInvoke => (),
};

const _: fn(marqdo_value::Value) = |value| match value {
    marqdo_value::Value::Unit
    | marqdo_value::Value::None
    | marqdo_value::Value::Bool(_)
    | marqdo_value::Value::Int(_)
    | marqdo_value::Value::Float(_)
    | marqdo_value::Value::Text(_)
    | marqdo_value::Value::List(_)
    | marqdo_value::Value::Map(_)
    | marqdo_value::Value::Table(_)
    | marqdo_value::Value::Formula(_)
    | marqdo_value::Value::Code(_)
    | marqdo_value::Value::FunctionRef(_)
    | marqdo_value::Value::PluginHandle(_)
    | marqdo_value::Value::Memory(_)
    | marqdo_value::Value::Resource(_) => (),
};

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

/// The outer match in the compatibility dispatcher is the operation catalogue.
#[test]
fn resource_request_operation_allowlist_is_reviewed() {
    let source = read(&repo_root().join("crates/marqdo-host/src/host/resource_requests.rs"));
    let dispatch = source
        .split_once("        match operation {\n")
        .expect("closed operation dispatch")
        .1
        .split_once("\n            _ =>")
        .expect("unknown operations must reach a rejection arm")
        .0;
    let mut names: Vec<_> = dispatch
        .lines()
        .filter_map(|line| line.strip_prefix("            "))
        .filter(|line| line.starts_with('"') || line.starts_with('|'))
        .flat_map(|line| line.split('"').skip(1).step_by(2))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "clock.now",
            "compress.gunzip",
            "compress.gzip",
            "env.get",
            "fs.append",
            "fs.exists",
            "fs.list",
            "fs.metadata",
            "fs.mkdir",
            "fs.read_at",
            "fs.remove",
            "fs.rename",
            "fs.walk",
            "process.run",
            "random.bytes",
            "regex.captures",
            "regex.match",
        ],
        "review changes under ADR-0062 and ADR-0064"
    );
}

/// RFC-EXT-0003 H-01: the CLI/host must not link the removed domain math
/// kernel and `resource_request` must not dispatch a math.* domain operation.
#[test]
fn domain_math_kernel_is_not_part_of_the_core() {
    let root = repo_root();
    for manifest in [
        "Cargo.toml",
        "crates/marqdo-host/Cargo.toml",
        "crates/cli/Cargo.toml",
    ] {
        let text = read(&root.join(manifest));
        assert!(
            !text.contains("marqdo-math"),
            "{manifest} must not depend on the removed domain math kernel"
        );
    }
    assert!(
        !root.join("crates/marqdo-math").exists(),
        "crates/marqdo-math was removed from the core repository"
    );
    let host = read(&root.join("crates/marqdo-host/src/host.rs"));
    for removed in [
        "MathEval",
        "MathParse",
        "MathSubstitute",
        "MathSimplify",
        "MathDifferentiate",
    ] {
        assert!(
            !host.contains(removed),
            "host.rs still dispatches {removed}"
        );
    }
}

/// E02/E03: there is exactly one Host trait/HostOp definition and it belongs to
/// the neutral contract; the native host only implements it.
#[test]
fn host_contract_is_the_single_side_effect_interface() {
    let root = repo_root();
    let mut host_traits = Vec::new();
    let mut host_ops = Vec::new();
    for source in walk_rs(&root.join("crates")) {
        let relative = source
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if relative.contains("/tests/") {
            continue;
        }
        let text = read(&source);
        if text.contains("pub trait Host {") {
            host_traits.push(relative.clone());
        }
        if text.contains("pub enum HostOp {") {
            host_ops.push(relative);
        }
    }
    assert_eq!(
        host_traits,
        ["crates/marqdo-host-contract/src/host.rs"],
        "the Host trait must be defined once in the neutral contract"
    );
    assert_eq!(
        host_ops,
        ["crates/marqdo-host-contract/src/host.rs"],
        "HostOp must be defined once in the neutral contract"
    );
    let runtime = read(&root.join("crates/marqdo-runtime/src/interpreter.rs"));
    assert!(
        runtime.contains("use marqdo_host_contract::{"),
        "the interpreter must consume the neutral contract"
    );
    let host = read(&root.join("crates/marqdo-host/src/host.rs"));
    assert!(
        host.contains("impl Host for StdHost"),
        "StdHost remains the single native host implementation"
    );
}

/// A05/S01: the public ABI crate is a contract, not a loader; execution side
/// lives in the native host and the SDK consumes only the contract.
#[test]
fn plugin_contract_carries_no_native_execution() {
    let root = repo_root();
    let abi = read(&root.join("crates/marqdo-plugin-abi/Cargo.toml"));
    for forbidden in ["libloading", "marqdo-worker-ipc", "serde_yaml", "sha2"] {
        assert!(
            !abi.contains(forbidden),
            "marqdo-plugin-abi must not depend on execution-side `{forbidden}`"
        );
    }
    for module in ["loader", "worker", "manifest", "artifact"] {
        assert!(
            !root
                .join(format!("crates/marqdo-plugin-abi/src/{module}.rs"))
                .exists(),
            "`{module}` is execution-side and belongs to the native host"
        );
    }
    for module in ["loader", "worker", "manifest", "artifact"] {
        assert!(
            root.join(format!("crates/marqdo-host/src/plugin/{module}.rs"))
                .exists(),
            "native host must own plugin `{module}`"
        );
    }
    let sdk = read(&root.join("crates/marqdo-plugin-sdk/Cargo.toml"));
    for forbidden in [
        "marqdo-host",
        "marqdo-cli",
        "libloading",
        "marqdo-worker-ipc",
    ] {
        assert!(
            !sdk.contains(forbidden),
            "the plugin SDK must not depend on `{forbidden}`"
        );
    }
}

/// E01/E04: the independent embedding consumer is a real package outside the
/// workspace and depends only on engine crates.
#[test]
fn embedding_consumer_is_engine_only() {
    let root = repo_root();
    let consumer = read(&root.join("tests/embedding/Cargo.toml"));
    assert!(
        consumer.contains("[workspace]"),
        "the embedding consumer must be its own workspace to avoid feature unification"
    );
    assert!(
        consumer.contains("marqdo-runtime"),
        "the embedding consumer must execute through the real runtime"
    );
    for forbidden in [
        "marqdo-host",
        "marqdo-cli",
        "marqdo-sandbox",
        "marqdo-foreign",
    ] {
        let dependency_line = format!("\n{forbidden} =");
        assert!(
            !consumer.contains(&dependency_line),
            "the embedding consumer must not depend on `{forbidden}`"
        );
    }
    assert!(
        root.join("tests/embedding/Cargo.lock").is_file(),
        "the independent consumer must pin its own lock file"
    );
    assert!(
        root.join("tools/ci/check_engine_boundary.py").is_file(),
        "the build-and-run engine gate must exist"
    );
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
