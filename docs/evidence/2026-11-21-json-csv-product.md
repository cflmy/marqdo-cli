# Evidence: `std:data/json` and `std:data/csv`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `fbe36bcc431ccadcb95b5aba74ecd7c39113f9ae`
Repository: `marqdo-cli`
RFC item: P1 JSON/CSV product stdlib APIs

## Implemented

`stdlib/data/json.mq.md` is a recursive JSON parser/formatter written only in
`.mq.md`:

- `解析(文本)` -> `{ok:true, value}` or `{ok:false, code, message}`;
  rejects malformed numbers, strings, escapes, arrays, objects and trailing
  input (`json.trailing`).
- `格式化(值)` -> canonical text: strings escaped, lists/objects comma-space
  separated, `None` as `null`, Text/Int/Float/Bool by their value form.

`stdlib/data/csv.mq.md` is a quoted single-row CSV parser/formatter:

- `解析行(文本)` -> `{ok:true, value:List<Text>}`; `""` is a literal quote;
  unterminated quotes and a quote inside an unquoted field are recoverable
  `csv.unterminated_quote`/`csv.invalid_quote`.
- `格式化行(列表)` -> quotes fields containing comma/quote/CR/LF and doubles
  inner quotes.

No Rust builtin, `HostOp` or parser change was added. Both modules are pure
`.mq.md` value functions.

## Tests

`stdlib/tests/json.mq.md` and `stdlib/tests/csv.mq.md` run through
`std:data/json` and `std:data/csv` and are pinned by
`crates/cli/tests/stdlib_product.rs` (8 product tests total).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                469 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test stdlib_product            8 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No streaming/incremental JSON, no depth bound (deep nesting can exhaust the
interpreter stack), no multi-line CSV fields, no duplicate-key policy beyond
last-write-wins, and no `std:data/json` conversion to `Table`. Those remain
open product work.
