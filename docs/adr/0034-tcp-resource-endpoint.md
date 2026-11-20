# ADR-0034: Blocking TCP resource endpoint v1 (CM-A09/P5 first slice)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A11, P5), ADR-0025, ADR-0033
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-tcp-endpoint.md`

## Context

CM-A09/P5 require a real external endpoint reached through the stable resource
ABI, not a new `HostOp` or Rust builtin per application. The in-process
provider slice (ADR-0033) covers trusted adapters; it does not exercise a real
OS byte stream. This ADR adds the first OS endpoint: a blocking TCP stream.

The scope is deliberately narrow: HTTP/1.1, TLS, DNS, UDP, sockets as a broad
API and nonblocking/async I/O remain standard-library or later work.

## Decision

### 1. Resource kind and descriptor

```text
resource.open("tcp", "<ip-literal>:<port>", ["read", "write"]) -> Resource
```

* descriptor must parse as a `SocketAddr` containing an **IP literal**. DNS
  names are rejected with `resource.unsupported`; no resolver is called.
* rights must be exactly `["read","write"]` in this build. A read-only or
  write-only TCP handle is future work.
* `resource.read`/`write`/`close` behave like other streams:
  * read returns `{count, eof}`; `count=0` is transport EOF;
  * write returns a possibly short count;
  * close calls `shutdown(Both)` and marks the handle closed;
  * closed/forged/stale handles use the existing `resource.closed` /
    `runtime.resource` rules.
* connect, read and write have 5-second timeouts. This is blocking I/O on the
  interpreter thread; no async runtime is introduced.

### 2. Authorization

* `project` mode only. `pure` and `strict` refuse TCP (`permission.denied`);
  strict remains fail-closed (CM-A11), and the G-C lane verifies the refusal.
* `Capabilities.check_net(ip)` requires an exact IP-literal grant. The CLI adds
  `--allow-net <IP>` (repeatable); `--allow-net` is refused alongside other
  external grants in pure/strict preflight.
* Granting a hostname or a network range does not authorize a different IP;
  there is no DNS-based allowlist bypass because DNS is not used.
* No TLS, no proxy, no HTTP parsing in the host. `.mq.md` code composes
  protocols over the byte stream.

### 3. Trace

`resource.open/read/write/close` effect events are recorded. The trace records
the IP:port for open but never payload bytes or credentials. A TCP endpoint is
an external effect and is not replayed as pure computation.

### 4. Explicit non-goals

* DNS, TLS, HTTP/1.1 or HTTP/2 host implementations, proxies, redirects.
* UDP, ICMP, raw sockets, socket options, nonblocking/async/select.
* File descriptors exposed to the language, `setsockopt`, `bind`/`listen` server
  sockets.
* Strict-mode TCP and any claim that in-process `std::net` is OS isolation.
* Network provider package installation or a generic socket registry.

## Consequences

* `.mq.md` can implement a line protocol/HTTP client parser on top of the TCP
  byte stream using the existing Memory/Text/Result primitives. The bundled
  `stdlib/net/http.mq.md` provides HTTP/1.1 response parsing (status, headers,
  chunked transfer) and request construction; `http_over_tcp.rs` runs a local
  HTTP exchange through `resource.open("tcp", ...)` and the module.
* The runtime still has no `net.request` builtin or new `HostOp`; a CLI test
  opens a local echo server, writes/reads `ping` through `resource.*`, and
  closes.
* Blocking timeouts bound a hung peer but do not preempt a blocking syscall
  deeper than the OS timeout; this is documented.
* Network tests use loopback only; external network access is not required for
  the gate.

## Verification

`docs/evidence/2026-11-20-tcp-endpoint.md` records host tests (loopback echo,
grant/pure/strict/DNS refusal) and a CLI `.mq.md` echo test with
`--allow-net 127.0.0.1`, plus the full workspace gate.
