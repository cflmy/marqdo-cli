# ADR-0040: Stdio stdin/stderr endpoints and stdlib stream APIs

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0025, ADR-0026, ADR-0027, ADR-0030, ADR-0031
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-21-stdio-streams.md`

## Context

ADR-0025 opened one `stdio/stdout` write endpoint. That left `stdin` and
`stderr` as hidden OS channels, and the bundled `io.写文本` ignored the
`resource.write` `{ok,count}` result. The audit RFC calls for a real resource
boundary, not a Rust `Input`/`Print` builtin.

## Decision

* Add `stdio/stdin` (rights `["read"]`) and `stdio/stderr` (rights
  `["write"]`). `stdio/stdout` remains `["write"]`. Rights/kinds are exact;
  mixing them is `resource.unsupported`.
* All stdin handles in one run share one cursor. `resource.read` consumes bytes and
  reports `{count,eof}`; EOF is a successful zero-byte read, not an error.
  Closing a handle does not reset or close the run cursor. There is no blocking
  OS read inside the host table.
* stdout and stderr are distinct host buffers. Both charge the existing output
  budget, but `output()`/`take_output()` never return stderr and
  `error()`/`take_error()` never return stdout. A budget failure appends
  nothing to either buffer.
* The CLI reads process stdin once at composition time only when stdin is not a
  terminal. This keeps an interactive `marqdo run` from waiting for EOF. JSON run
  output adds a separate `stderr` string; text mode writes captured program
  stderr to process stderr. Diagnostics and traces remain CLI stderr and never mix
  with the JSON stdout document.
* Strict mode remains fail-closed: only `stdio/stdout` with `["write"]` and the
  console grant is allowed. stdin, stderr, file, TCP and provider streams are
  `permission.denied`.
* The standard library keeps the product APIs in `.mq.md`:
  `stdlib/io/stream.mq.md` owns `写全部`/`写文本` with short-write and close
  checking; `stdlib/io/console.mq.md` owns `读取行`/`打印` over the resource
  ABI; `stdlib/text/utf8.mq.md` owns a validating UTF-8 decoder.
  `stdlib/io.mq.md` remains the compatibility facade and print target.

## Non-goals

No process, device, directory, DNS, TLS, UDP, seek, mmap, async or zero-copy
endpoint. The stdin source is the finite process input supplied by the
composition root, not a general interactive console driver. No new language
keyword, `Input` builtin, `HostOp::Print` or second Markdown parser.

## Compatibility

The resource ABI is additive. Existing stdout-only programs, `打印`, file
read/write and provider/tcp streams keep their prior behavior. A program that
used a closed handle still gets the recoverable `resource.closed` result.

## Consequences

CLI programs can now read piped stdin and write program stderr without leaking
either into JSON diagnostics. The remaining OS-shaped gaps are explicit and can be
added only through a separate resource operation decision.
