# ADR-0047: DNS resolution, UDP and TCP listener endpoints

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0034, ADR-0041
Specs: `spec/security.md`, `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-net-endpoints.md`

## Context

ADR-0034 opened only an outbound IP-literal TCP stream. That left DNS names,
datagrams and inbound listeners unavailable, and the `--allow-net` grant was
documented as IP-only. The audit RFC's P2 network gap requires real endpoints,
not a `TODO`.

## Decision

* `resource.open("tcp", "host:port", ["read","write"])` now accepts a DNS name
  or IP literal. The **host** (not the resolved IP) is checked against the
  exact `--allow-net HOST` grant **before** `ToSocketAddrs`; every resolved
  address is tried with a 5 s connect timeout.
* Add `resource.open("udp", "host:port", ["read","write"])`: a connected
  `UdpSocket` from an ephemeral local port. `resource.read` returns one
  datagram (truncated to the destination `Memory` bound, `eof=false`);
  `resource.write` sends one datagram.
* Add `resource.open("tcp-listener", "host:port", ["accept"])` plus the
  `resource.accept(listener)` builtin. `accept` polls a non-blocking listener
  for up to 5 s, returns a new TCP `Resource` in `{ok:true, value:Resource}`,
  or `{ok:false, code:"tcp.timeout"}`/`tcp.accept` on failure.
* All three are project-mode only. `pure`/`strict` return `permission.denied`;
  the listener still requires the exact host grant.
* Host names are accepted but **not** pinned to an IP: a DNS answer may change
  between grant and connect. No TLS, certificate validation, server sockets
  outside project mode or UDP broadcast/multicast is added.
* The grant flag remains `--allow-net HOST` (repeatable); its help text now says
  host name or IP literal.

## Consequences

* DNS, UDP and inbound TCP are reachable from `.mq.md` without a new builtin
  per transport. `resource.accept` is the only new builtin and returns an
  opaque handle like `resource.open`.
* TLS still needs a separate decision (certificate validation and trust
  anchors); this ADR does not claim TLS.
* `resource.read` on UDP is datagram-based; a zero-length datagram is a
  successful empty read, and there is no stream EOF.
