# Shared persistent-worker transport

Status: **implemented pre-stable internal transport**, owner crate
`crates/marqdo-worker-ipc`, introduced by
[ADR-0068](../docs/adr/0068-single-current-public-contracts.md) for issue #8.

This document is not a public product protocol and has no `schema_version`.
It describes the one implementation of the framing and process plumbing that
the two persistent worker boundaries — the plugin worker
([plugin ABI](plugin-abi.md) §9) and the resource-provider worker
([resource provider](resource-provider-v2.md) §5) — use instead of maintaining
private copies.

## 1. What is shared, and what is not

Shared (`crates/marqdo-worker-ipc`):

- the four-byte big-endian length prefix followed by one JSON value;
- maximum-frame bounding supplied by each caller (8 MiB plugins, 1 MiB
  providers) and rejection of zero/oversized lengths;
- clean EOF at a frame boundary versus a partial header, truncated payload or
  invalid JSON;
- one reader thread and one writer thread per child, so a caller never reads
  and writes the same pipe concurrently;
- caller-supplied exchange deadlines, process kill/reap on any transport
  failure, and "a dead worker is never reused";
- structured transport errors (`Timeout`, `Closed`, `Frame`).

Not shared — each boundary keeps its own:

- frame vocabulary and state machine (`ready`/`invoke`/`capability_*` versus
  `ping`/`open`/`read`/`write`/`close`/`shutdown`);
- frame size limit and capability/budget policy;
- authorization, grants, kind/rights checks and lifecycle;
- error codes and public observation.

A shared transport never shares capabilities, trust or identity. Killing a
worker after a transport failure is plumbing; deciding what a worker may do is
a separate gate.

Foreign kernels and CLI Tools are deliberately excluded. A foreign kernel is a
one-shot process whose stdout must be exactly one value-wire object
([foreign](foreign.md)); a CLI Tool is an independent executable that owns its
standard streams ([CLI tool](cli-tool.md)). Neither has a persistent framed
channel, and neither may gain one by reusing this crate.

## 2. Framing rules

```text
+--------------------------+---------------------------+
| 4-byte big-endian length | exactly `length` bytes    |
| (1 ..= caller limit)     | of one JSON value         |
+--------------------------+---------------------------+
```

| Outcome | Classification |
|---|---|
| EOF before any header byte | clean `None` (not an error) |
| EOF inside the four-byte header | `FrameError::PartialHeader` |
| length `0` or above the caller limit | `FrameError::Length` |
| EOF inside the payload | `FrameError::PartialPayload` |
| payload is not UTF-8 JSON | `FrameError::InvalidJson` |
| write larger than the caller limit | `FrameError::Length` (nothing written) |

Writers flush every frame. Readers bound the allocation before reading. There
is no negotiation, no version field and no fallback: each boundary has exactly
one current frame shape at its existing format identifier.

## 3. Exchange and deadline rules

The host-side `Connection` is sequential: one exchange is in flight at a time.
The caller passes an `Instant` deadline with `send`/`receive`; a missed deadline
kills and reaps the child and returns `TransportError::Timeout`. Clean EOF
before a reply returns `Closed`; a malformed frame returns `Frame` with the
classification above. Every transport error aborts the connection, so a worker
that timed out, crashed or violated framing is never reused for a later call.

The worker side uses the same `read_frame`/`write_frame` functions on its own
stdin/stdout. Worker processes still exit non-zero on malformed input rather
than guessing at a retired format.

## 4. Boundary-specific deadlines (current values)

| Boundary | Handshake | One exchange | Quarantine |
|---|---|---|---|
| Plugin worker | 30 s | 10 s per invocation, covering capability round-trips | killed, `plugin.timeout` |
| Resource-provider worker | 30 s (ping) | 30 s per request | killed, host error |

Timeouts classify a failed exchange; they do not change program semantics.
The strict sandbox decision is still made by the OS isolation adapter, never by
the fact that a worker is a child process.

## 5. Verification

`crates/marqdo-worker-ipc` owns the transport tests: framing round-trip,
boundary/EOF classification, oversize rejection, a live two-frame exchange, a
silent child hitting its deadline and being reaped, clean EOF, and a truncated
payload. The plugin and provider suites keep testing their own vocabulary,
grants, token lifecycle and error codes through the real worker binaries, so a
transport refactor cannot hide a behaviour change at either boundary.
