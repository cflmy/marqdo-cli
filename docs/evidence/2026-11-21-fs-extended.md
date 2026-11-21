# Evidence: `fs.append`, `fs.read_at` and `fs.walk`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `7859c26213bda30b34a5379a4ab1f75430967494`
Repository: `marqdo-cli`
Decision record: [ADR-0045](../adr/0045-fs-append-read-at-walk.md)

## Implemented

`resource.request` now supports:

- `fs.append(path, text)` -> `{ok:true, value:count}`; creates the file,
  requires `fs_write`, 8 MiB cap.
- `fs.read_at(path, offset, length)` -> `{ok:true, value:Text}`; non-negative
  integers, 8 MiB cap, invalid UTF-8 is fatal.
- `fs.walk(dir)` -> `{ok:true, value:List<Text>}`; recursive regular files,
  skips symlinks, depth 64, 4096-entry cap.

`std:fs/file` exposes `追加文本`, `读取片段`, `遍历`.

## Tests

`stdlib/tests/fs.mq.md` appends, reads a slice and walks the output directory;
`crates/marqdo-host/tests/resource_streams.rs` pins host behavior and the
negative-offset type error.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                479 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        24 passed
cargo test -p marqdo-cli --test stdlib_product           14 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No file watch, no binary offset read into `Memory`, no seek/resume handle, no
write-at-offset, no recursive delete and no symlink traversal.
