# Evidence: byte-exact HTTP/1.1 framing

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `28b178fc70d317ca860b922f861456b7c0034a95`
Repository: `marqdo-cli`
Decision record: [ADR-0051](../adr/0051-byte-exact-http-framing.md)

## Implemented

`stdlib/net/http.mq.md` now parses and builds byte lists:

- `解析响应字节` -> `{ok:true,value:{status,reason,headers,body}}`, body raw
  bytes, chunk sizes as hexadecimal **bytes**;
- `文本字节`/`字节文本` UTF-8 boundary helpers;
- `构建请求字节` with raw body bytes;
- `解析响应`/`构建请求` remain compatible text wrappers.

## Tests

`stdlib/tests/http.mq.md` checks a chunked body containing UTF-8 `你好`
(length 6 bytes) and a binary request body `[65,66]`; the text result is still
`hello world` for the original ASCII case. `stdlib_http.rs` pins status,
reason, header, body, byte-body text, and request bytes.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                491 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test stdlib_http              1 passed
cargo test -p marqdo-cli --test stdlib_product           16 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No HTTP/2, header folding, trailers, gzip transfer decoding, streaming
message framing or request/response Resource adapter.
