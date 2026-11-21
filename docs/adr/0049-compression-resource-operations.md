# ADR-0049: gzip compression resource operations

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-compress-product.md`

## Context

Compression is pure computation, but implementing gzip in `.mq.md` would either
duplicate a large algorithm or under-deliver. Adding a new Rust builtin would
expand the language surface; the existing non-streaming `resource.request`
channel is a better fit for an explicitly bounded codec operation.

## Decision

Add two project-mode operations:

```text
resource.request("compress.gzip",   [text])      -> {ok:true, value:List<Int>}
resource.request("compress.gunzip", [bytes])     -> {ok:true, value:Text}
                                                   | {ok:false, code:"compress.invalid"}
                                                   | {ok:false, code:"compress.not_utf8"}
                                                   | {ok:false, code:"compress.too_large"}
```

* `gzip` takes Text, caps input at 8 MiB, writes a deterministic gzip header
  (no filename/time) and returns a byte list.
* `gunzip` takes a `List<Int>` of bytes, caps input and output at 8 MiB,
  returns Text on success, and maps bad gzip/UTF-8/oversize output to the
  recoverable codes above.
* No new capability flag is added: compression is deterministic CPU work and
  does not disclose external state. It remains project-mode only because all
  `resource.request` operations do; `pure`/`strict` refuse it.
* `flate2` (miniz_oxide backend) is the implementation. No new builtin,
  `HostOp`, parser node or wire value is added.
* `stdlib/compress.mq.md` exposes `压缩`/`解压`.

## Consequences

* Callers can compress data without a second language surface, but must pass
  byte lists through ordinary `List<Int>` values.
* Streaming gzip, raw deflate, zstd/brotli and binary `Memory` output remain
  open. A streaming codec would need a new Resource kind.
