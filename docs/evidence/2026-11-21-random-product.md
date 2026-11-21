# Evidence: `std:random` and `random.bytes`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `6338fa10e948d9f62f8eea84c793a0c4255e8d66`
Repository: `marqdo-cli`
Decision record: [ADR-0046](../adr/0046-random-resource-operation.md)

## Implemented

`resource.request("random.bytes", [count])` is project-mode only:

- count 1..=65536; other values are fatal type errors;
- exact `--allow-random` grant required; pure/strict refuse;
- bytes come from `getrandom::fill` and are returned as `List<Int>` 0–255;
- one `resource.request` trace/effect event.

`stdlib/random.mq.md` exposes `字节(n)` and `整数(上限)` (8 bytes modulo
upper bound; not claimed unbiased).

## Tests

`stdlib/tests/random.mq.md` runs with `--allow-random` and checks length and
range. `crates/marqdo-host/tests/resource_streams.rs` pins grant denial,
pure-mode refusal, byte range and count validation (25 host tests).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                481 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        25 passed
cargo test -p marqdo-cli --test stdlib_product           15 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No unbiased rejection sampling, token/secret API, deterministic seed source or
cryptographic post-processing. The byte list is plain OS entropy.
