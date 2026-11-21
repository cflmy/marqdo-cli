# Evidence: non-streaming Resource Operation (`fs.*`)

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `cd90f059b4a91993f89fba5ae63b400dac4bd2c4`
Repository: `marqdo-cli`
Decision record: [ADR-0041](../adr/0041-resource-operation-fs.md)
RFC item: P1 file metadata/directory operations

## Implemented

`resource.request(operation, arguments)` adds one builtin and one host trait
method. The first operation group is `fs.*`:

| Operation | Arguments | Grant | Recoverable failures |
|---|---|---|---|
| `fs.exists` | path | `fs_read` | — |
| `fs.metadata` | path | `fs_read` | `fs.not_found` |
| `fs.list` | directory | `fs_read` | `fs.not_found`, `fs.not_directory` |
| `fs.mkdir` | path | `fs_write` | `fs.exists`, `fs.not_found` |
| `fs.remove` | path | `fs_write` | `fs.not_found`, `fs.not_empty` |
| `fs.rename` | from, to | `fs_write` on both | `fs.not_found`, `fs.exists` |

`pure`/`strict` return `permission.denied`; traversal, absolute paths,
unknown operations and I/O failures stay fatal. `remove` never recurses and
`rename` never overwrites. No file bytes move through this ABI.

`stdlib/fs/file.mq.md` now exposes `存在`, `元数据`, `是文件`, `是目录`,
`列出目录`, `创建目录`, `删除` and `重命名` over `resource.request`, still in
`.mq.md`.

## Commands

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                467 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        20 passed
cargo test -p marqdo-cli --test stdlib_product            6 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No append/seek/watch, recursive removal or recursive directory walk. No
process/device/DNS/TLS/UDP operation. Those need a later ADR and capability
group. `fs.exists`/metadata follow symlinks only when the canonical target
stays inside the project root.
