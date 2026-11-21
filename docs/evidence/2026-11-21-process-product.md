# Evidence: `std:process` and the `process.run` grant

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `9922bfae98d72f7dbad26f802fd3cc9478272b8e`
Repository: `marqdo-cli`
Decision record: [ADR-0044](../adr/0044-process-resource-operation.md)
RFC item: P2 process product API

## Implemented

`resource.request("process.run", [program, args])` is project-mode only:

- exact Text program path and List-of-Text arguments; no shell/PATH/glob;
- exact `--allow-process PROGRAM` grant checked before spawn;
- child has `env_clear`, stdin null, project-root cwd, 5 s timeout (kill) and
  separate 1 MiB stdout/stderr caps;
- recoverable `process.not_found`, `process.timeout`,
  `process.output_too_large`; un-granted programs are `permission.denied`;
  non-UTF-8 output is a fatal type error.

`stdlib/process.mq.md` is a thin `.mq.md` facade: `运行(程序, 参数列表)`.
The child is **not** an OS sandbox and inherits the CLI user's privileges.

## Tests

`stdlib/tests/process.mq.md` runs `/bin/echo hello` with
`--allow-process /bin/echo`. `crates/marqdo-host/tests/resource_streams.rs`
pins grant denial, pure-mode refusal, exit status/stdout/stderr and the
missing-program grant check.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                478 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        23 passed
cargo test -p marqdo-cli --test stdlib_product           14 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

Not a sandbox; no strict-mode process provider, no stdin, no shell operators,
no environment selection, no per-run user/namespace isolation, no output
streaming. A sandboxed process provider needs its own ADR and OS evidence.
