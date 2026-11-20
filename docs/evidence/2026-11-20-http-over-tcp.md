# RFC-CM P5/C10 evidence: HTTP/1.1 over the TCP resource endpoint

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `0d4dc1d` plus this step's changes.
Related: [ADR-0034](../adr/0034-tcp-resource-endpoint.md),
[HTTP parser fixture](2026-11-20-http-parser.md), [STATUS §5](../STATUS.md).
Scope: `.mq.md` HTTP framing over a loopback TCP stream; no host HTTP/TLS.

## Module

`stdlib/net/http.mq.md` (import `std:net/http`) provides:

```text
解析响应(文本)          -> {状态, 原因, 头, 体}
解码分块(体)            -> Text
构建请求(方法, 路径, 头列表, 体) -> Text
切片/找子串/找字符/十六进制值
```

It uses only `text.codepoints`/`text.from_codepoint` and List/Map/Text values;
there is no `net.request` or host HTTP parser.

## End-to-end test

`crates/cli/tests/http_over_tcp.rs`:

1. starts a loopback `TcpListener` that reads the request and sends
   `HTTP/1.1 200 OK ... Content-Length: 5 ... hello`;
2. runs a generated `.mq.md` program with `--allow-net 127.0.0.1`;
3. the program builds a GET request, UTF-8-encodes it through `std:text` into
   `Memory`, writes it to `resource.open("tcp", addr, ["read","write"])`,
   reads the response bytes, decodes them to Text with
   `text.from_codepoint`, parses with `stdlib/net/http`, and returns
   `[write.ok, read.ok, status, content-type, body, close.ok]`;
4. asserts `[true, true, 200, "text/plain", "hello", true]`.

`stdlib/tests/http.mq.md` independently pins the parser/builder without a
socket. Semantic fixture 71 pins the same logic in the conformance corpus.

## Gate

Local container: `cargo test -p marqdo-cli --test http_over_tcp` PASS and
`cargo test -p marqdo-cli --test stdlib_http` PASS; full workspace gate PASS.
No external network access is required.

## Not done

* HTTPS/TLS; no TLS security claim.
* External DNS, redirects, connection reuse, compression, cookies, HTTP/2.
* A host HTTP client/server; this is `.mq.md` framing over the byte stream.
* Strict-mode TCP (strict refuses TCP; project mode only).
