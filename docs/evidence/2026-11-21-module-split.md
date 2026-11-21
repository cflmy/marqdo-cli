# Evidence: CLI/Host responsibility split

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `3ef18fb496096d97e3c48ff661f0b2ad00cdcdc3`
Repository: `marqdo-cli`
RFC item: P1 module boundaries

## Moved

| New module | Responsibility |
|---|---|
| `crates/cli/src/commands/sandbox.rs` | `SandboxSettings`, mode parsing, strict preflight and CLI-to-`Capabilities` mapping; `commands.rs` re-exports it |
| `crates/marqdo-host/src/host/resource_ops.rs` | `open_resource_stream` stdio/file/tcp/provider dispatch; child module of `host` |
| `crates/marqdo-host/src/process.rs` | explicitly granted `process.run` execution, timeout and output caps |

This is a **behavior-preserving** refactor: no public CLI flag, host trait
method, builtin, spec or stdlib API changed. No new crate was added. Provider
registration and `commands.rs` run/check/inspect still live in their original
files.

## Commands

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                478 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test docs_consistency          5 passed
cargo test -p marqdo-cli --test architecture_boundaries    4 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

`commands.rs` still contains run/check/inspect/package/tool command bodies
and `host.rs` still contains provider registration and capability dispatch.
A complete split needs a later behavior-preserving change; it is not claimed
here.
