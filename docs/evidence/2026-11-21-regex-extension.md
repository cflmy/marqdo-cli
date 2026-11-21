# Evidence: host-backed regex extension

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `94a13880c417739d1d8cc270097c376ef69d87df`
Repository: `marqdo-cli`
Decision record: [ADR-0050](../adr/0050-host-regex-extension.md)

## Implemented

- `resource.request("regex.match", [pattern, text])` -> Bool.
- `resource.request("regex.captures", [pattern, text])` -> `{matched, groups}`
  with group 0 and `None` for unmatched groups.
- Invalid patterns are recoverable `regex.invalid`; no new grant; project mode
  only.
- `stdlib/regex.mq.md` exposes `扩展匹配`, `扩展搜索`, `捕获`; pure
  `匹配`/`搜索` subset unchanged.

## Tests

`stdlib/tests/regex.mq.md` covers `^(a+)(b|c)$` matching/captures.
`crates/marqdo-host/tests/resource_streams.rs` pins groups/alternation,
captures and invalid-pattern recovery (34 host tests).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                491 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        34 passed
cargo test -p marqdo-cli --test stdlib_product           16 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No pure-mode groups/alternation/captures, look-around, backreferences or
regex replace/split. The host engine is only reachable in project mode.
