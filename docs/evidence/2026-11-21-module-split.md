# Evidence: CLI/Host responsibility split

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `791819a72576f54e406e7dfbbe6ac635f9b1edd2`
Repository: `marqdo-cli`
RFC item: P1 module boundaries

## Moved

| New module | Responsibility |
|---|---|
| `crates/cli/src/commands/sandbox.rs` | `SandboxSettings`, mode parsing, strict preflight and CLI-to-`Capabilities` mapping |
| `crates/cli/src/commands/run.rs` | `execute` run-session assembly and `read_stdin` |
| `crates/cli/src/commands/check.rs` | `check` command |
| `crates/cli/src/commands/inspect.rs` | `inspect` command and `InspectFlags` |
| `crates/marqdo-host/src/host/resource_ops.rs` | `open_resource_stream` stdio/file/tcp/provider dispatch; child module of `host` |
| `crates/marqdo-host/src/host/providers.rs` | trusted in-process/worker/C-ABI/package provider registration |
| `crates/marqdo-host/src/host/effects.rs` | `next_seq` and `record_resource_effect` trace helpers |
| `crates/marqdo-host/src/process.rs` | explicitly granted `process.run` execution, timeout and output caps |

This is a **behavior-preserving** refactor: no public CLI flag, host trait
method, builtin, spec or stdlib API changed. No new crate was added.
`commands.rs` still owns init/fmt/lock/sync/package/tool and `host.rs` still
owns `Host::invoke` dispatch and capability checks.

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

`commands.rs` still contains init/fmt/lock/sync/package/tool command bodies
and `host.rs` still contains `Host::invoke` dispatch, capability checks and
register entry points. A complete split needs a later behavior-preserving
change; it is not claimed here.
