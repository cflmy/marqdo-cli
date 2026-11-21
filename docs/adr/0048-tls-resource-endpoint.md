# ADR-0048: TLS client resource endpoint

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0034, ADR-0047
Specs: `spec/security.md`, `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-tls-endpoint.md`

## Context

ADR-0047 added DNS, UDP and TCP listeners but deliberately left TLS out: a TLS
client needs a trust-anchor policy and certificate validation, not just a
socket. The audit RFC lists TLS as a P2 provider gap.

## Decision

* `resource.open("tls", "host:port", ["read","write"])` connects with TCP,
  validates the server certificate with `rustls`, and returns an opaque
  `tls` Resource. `resource.read`/`write`/`close` work like TCP.
* The exact host grant (`--allow-net HOST`) is checked before DNS resolution.
  `pure`/`strict` refuse; only project mode is supported.
* Trust roots are the platform native roots plus any DER certificates added by
  an embedder through `StdHost::with_tls_ca`. There is no CLI flag for a private
  CA in this build; the API is for embedders/tests.
* Server-name verification uses the descriptor host, not the resolved IP.
  Certificate or handshake failures are fatal `host.error`; a failed handshake
  closes the connection and does not return a Resource.
* TLS is client-only. No server TLS acceptor, mutual TLS, ALPN, session
  resumption, OCSP or custom root CLI is added.
* `rustls` is used instead of `native-tls` so the build does not require
  system OpenSSL headers; it remains a normal Rust dependency.

## Consequences

* `resource.open("tls")` is a real validated TLS client endpoint and is covered
  by a local self-signed test that adds an explicit DER root.
* A future CLI `--tls-ca` or provider-managed trust store needs a separate
  grant/ADR; this decision does not silently trust unknown certificates.
* TLS server sockets and mutual TLS remain open.
