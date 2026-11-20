# RFC-CM P5/C10 evidence: `.mq.md` HTTP/1.1 parser

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `67a6d35` plus this step's changes.
Related: [ADR-0032](../adr/0032-table-logical-value-and-stdlib-algorithms.md),
[ADR-0034](../adr/0034-tcp-resource-endpoint.md), [STATUS §5](../STATUS.md).
Scope: Text-level HTTP framing in `.mq.md`; no host HTTP/TLS implementation.

## Fixture

`tests/conformance/semantics/71-http-parser.mq.md` (+ expected golden):

* `找子串`/`找字符`/`切片` over `text.codepoints` and `text.from_codepoint`;
* `解析响应` splits the head/body at `\r\n\r\n`, parses the status line,
  headers and `Transfer-Encoding: chunked`;
* `解码分块` parses hex chunk sizes and reassembles the body;
* `构建请求(方法, 路径, 头列表, 体)` emits an HTTP/1.1 request with CRLF
  framing.

Input:

```text
HTTP/1.1 200 OK
Content-Type: text/plain
Transfer-Encoding: chunked

5
hello
6
 world
0
```

Result:

```json
[200, "OK", "text/plain", "hello world",
 "GET /health HTTP/1.1\r\nAccept: text/plain\r\nHost: example.test\r\n\r\n"]
```

The fixture is run end-to-end by `conformance_semantics` and is part of the
71 semantic goldens.

## Gate

Local container: fmt/clippy/426 tests PASS (one fixture test; the golden count
is 71). No host `net.request`, HTTP client/server or TLS code was added.

## Not done

* HTTP client over TCP, connection reuse, redirects, compression, cookies.
* HTTPS/TLS; this ADR/evidence makes **no** TLS security claim.
* Request-body streaming, trailers, chunked requests, HTTP/2.
* A reusable `stdlib/net/http.mq.md` module (the parser is a conformance
  fixture, not a bundled module yet).
* C/Worker provider transport and external stdlib repository publication.
