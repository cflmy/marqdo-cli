# Evidence: `std:datetime` and `std:clock`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `cffb6790bee49966226f5abc3be026e6c4e0f8f3`
Repository: `marqdo-cli`
Decision record: [ADR-0042](../adr/0042-clock-resource-operation.md)
RFC item: P2 datetime/clock product APIs

## Implemented

`stdlib/datetime.mq.md` is pure `.mq.md`: `格式化(毫秒)` emits UTC ISO-8601
(`YYYY-MM-DDTHH:MM:SS[.mmm]Z`) using the civil-from-days algorithm;
`解析(文本)` validates separators, digit components, ranges, leap years and
returns recoverable `datetime.invalid`. Pre-1970 negatives are supported.

`stdlib/clock.mq.md` is a thin facade: `现在()` ->
`resource.request("clock.now", [])`. The host adds `clock.now` to the
ADR-0041 operation channel: zero arguments, project mode only, UTC Unix
milliseconds, one `resource.request` trace/effect event. `pure`/`strict`
return `permission.denied`.

No new Rust builtin, `HostOp` or parser node was added.

## Tests

`stdlib/tests/datetime.mq.md` and `stdlib/tests/clock.mq.md` run through
`std:datetime`/`std:clock` and are pinned by
`crates/cli/tests/stdlib_product.rs` (12 product tests total).
`crates/marqdo-host/tests/resource_streams.rs` pins project-only access,
Unix-millis shape and zero-argument arity (21 host tests).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                474 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        21 passed
cargo test -p marqdo-cli --test stdlib_product           12 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No time zones, locale formatting, duration type, monotonic clock or
calendar arithmetic beyond UTC instants. No network time source.
