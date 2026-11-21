# Evidence: `std:compress`

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `ff3f19a2eaf9c2bd40a66335e682079b1d85dd9f`
Repository: `marqdo-cli`
Decision record: [ADR-0049](../adr/0049-compression-resource-operations.md)

## Implemented

`resource.request("compress.gzip", [text])` and
`resource.request("compress.gunzip", [bytes])` use `flate2`/miniz_oxide:

- gzip: Text -> List<Int>, 8 MiB input cap, deterministic header.
- gunzip: List<Int> -> Text, 8 MiB input/output caps; bad gzip/UTF-8/oversize
  output are recoverable `compress.invalid`/`compress.not_utf8`/
  `compress.too_large`.
- Project-mode only; no new grant or builtin.

`stdlib/compress.mq.md` exposes `压缩`/`解压`.

## Tests

`stdlib/tests/compress.mq.md` round-trips `hello 世界`.
`crates/marqdo-host/tests/resource_streams.rs` round-trips text and rejects
bad gzip (33 host tests).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                490 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        33 passed
cargo test -p marqdo-cli --test stdlib_product           16 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No streaming gzip, raw deflate, zstd/brotli, binary Memory output or
decompression of untrusted data beyond the 8 MiB output cap.
