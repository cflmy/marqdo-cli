# Evidence: JSON depth/multi-value and CSV multi-row

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `efbb8007e172be4662348d73903a2a320d9d6ff9`
Repository: `marqdo-cli`
RFC item: P1 JSON/CSV completeness

## Implemented

- `json.解析` now rejects nesting deeper than 128 with `json.depth`; the
  depth scanner ignores string contents and escapes.
- `json.解析多个(文本)` parses whitespace-separated JSON values and returns
  `{ok:true,value:List}`, failing atomically on the first bad value.
- `csv.解析(文本)` parses multi-row CSV with quoted fields, `""`, CRLF/LF and
  quoted newlines; `csv.格式化(行列表)` rebuilds the text.
- `csv.解析行`/`格式化行` remain the single-row API.

## Tests

`stdlib/tests/json.mq.md` covers 130-level input -> `json.depth` and
multi-value parsing; `stdlib/tests/csv.mq.md` covers quoted multi-row input
and formatting. Both run through `std:` imports in the product lane.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                490 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test stdlib_product           16 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No incremental/chunked JSON or CSV parser: both still require the full text in
one Text value. No streaming parser state/Resource kind.
