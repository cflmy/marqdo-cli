# Evidence: `std:path` and `std:regex`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `bde6f1d0ba6f3ab19829ea63e5e85f1df84979a4`
Repository: `marqdo-cli`
RFC item: P2 pure path/regex stdlib APIs

## Implemented

`stdlib/path.mq.md` (pure `.mq.md`):

- `连接(片段列表)` skips empty segments and inserts one `/`.
- `文件名`, `目录名`, `扩展名` split on the last `/` and `.`.
- `规范(路径)` lexically folds `.`, empty segments and `..`, keeps a leading
  `/`; it never touches the filesystem.
- `是否绝对(路径)`.

`stdlib/regex.mq.md` (pure `.mq.md`):

- `匹配(模式, 文本)` requires a full match; `搜索(模式, 文本)` tries every
  start position.
- Supports `^`, `$`, `.`, `*`, `+`, `?`, `[...]` ranges/negation and
  backslash escapes. Recursive token matcher with backtracking.
- Invalid patterns are recoverable `regex.invalid`; groups/alternation are
  `regex.unsupported`.

No Rust builtin or host operation was added.

## Tests

`stdlib/tests/path.mq.md` and `stdlib/tests/regex.mq.md` run through
`std:path`/`std:regex` and are pinned by
`crates/cli/tests/stdlib_product.rs` (10 product tests total).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                471 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test stdlib_product           10 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No regex groups, alternation, named classes (`\d`), Unicode property classes,
incremental matching or a compiled-pattern cache. No path I/O or symlink
resolution. Those remain open product work.
