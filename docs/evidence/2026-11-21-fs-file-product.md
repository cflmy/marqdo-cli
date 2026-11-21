# Evidence: `std:fs/file` product module

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `8dd3d5eb76e1acf6a7a015b69515933fb6bbf8e3`
Repository: `marqdo-cli`
Decision input: architecture audit and refactor RFC, P1 file API

## Implemented

`stdlib/fs/file.mq.md` exports three `.mq.md` functions over the existing
resource ABI:

| Function | Contract |
|---|---|
| `读文本(路径)` | `resource.open("file", path, ["read"])` -> one bounded 8 MiB read -> `utf8.解码` |
| `写文本(路径, 文本)` | UTF-8 encode into `Memory` -> `resource.write` -> `resource.close` flushes |
| `复制文件(源, 目标)` | bounded byte read from source -> bounded byte write to destination |

No Rust builtin, `HostOp` or parser change was added. Path checks, grants,
the 8 MiB read/write ceiling and close-flush behavior remain the host
resource ABI (ADR-0025/0026/0040). Metadata, `exists`, directory listing,
`mkdir`, `remove` and `rename` are **not** implemented; they need a Resource
Operation ADR.

## Tests

`stdlib/tests/fs.mq.md` runs through the real `std:fs/file` import with
`--allow-read out/*` and `--allow-write out/*`; it writes, reads back and
copies a UTF-8 file. `crates/cli/tests/stdlib_product.rs` pins the result
`[7, "你好\n", 7, "你好\n"]` and the bytes on disk. The test output directory
`stdlib/tests/out/` is ignored.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                464 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test stdlib_product           6 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

A caller can still open a `file` resource directly with the lower-level
builtins (that is the existing public ABI). Binary files up to the 8 MiB
ceiling are byte-exact for copy; text read requires valid UTF-8 by design.
