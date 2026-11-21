# ADR-0051: Byte-exact HTTP/1.1 framing

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0047
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-http-bytes.md`

## Context

The original `stdlib/net/http.mq.md` parsed Text codepoints. HTTP lengths are
octets: `Content-Length`, chunk sizes and bodies are byte counts. A non-ASCII
body changes the codepoint count and the parser was therefore wrong for
non-ASCII and binary payloads.

## Decision

Rewrite the HTTP module around byte lists (`List<Int>` 0–255):

```text
文本字节(text) -> List<Int>
字节文本(bytes) -> {ok,value:Text}
解析响应字节(bytes) -> {ok,value:{status,reason,headers,body:List<Int>}}
构建请求字节(method, path, headers, body:List<Int>) -> List<Int>
```

* The parser finds CRLF/header terminators as byte sequences, parses status and
  header names/values as UTF-8/ASCII, and keeps the body as raw bytes.
* `chunked` sizes are hexadecimal **byte** counts; `Transfer-Encoding` is
  matched on the decoded header value.
* Existing `解析响应(text)` and `构建请求(..., text)` remain compatible
  wrappers: they encode/decode UTF-8 at the boundary and return the same shape.
* All buffers are capped at 8 MiB by the existing `Memory`/`utf8` limits.
* No new Rust builtin, `HostOp` or grant is added; the module is pure `.mq.md`
  and works in pure/project/strict modes (it only needs `Memory` and
  `text` builtins).

## Consequences

* Non-ASCII chunked bodies and `Content-Length` now report byte lengths.
* Binary bodies can be inspected as byte lists; the text API still decodes
  UTF-8 and returns an empty body if the payload is invalid UTF-8.
* HTTP/2, header folding, trailers, gzip transfer decoding and streaming
  message framing remain open.
