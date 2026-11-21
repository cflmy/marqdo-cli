# Evidence: TLS client endpoint

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `ee0c6cc80a2782c4e88de63195eaf41a27077dcf`
Repository: `marqdo-cli`
Decision record: [ADR-0048](../adr/0048-tls-resource-endpoint.md)

## Implemented

`resource.open("tls", "host:port", ["read","write"])` is a project-mode
validated TLS client:

- exact host grant before DNS;
- TCP connect with 5 s timeout, then `rustls` handshake using platform native
  roots plus `StdHost::with_tls_ca` DER roots;
- server name is the descriptor host; certificate/handshake failure is fatal
  `host.error`;
- the resulting `tls` Resource supports `resource.read/write/close`;
- no TLS server, mTLS, ALPN or custom-CA CLI.

## Tests

`crates/marqdo-host/tests/resource_streams.rs` starts a local `rustls` server
with an `rcgen` self-signed certificate, adds that DER as an explicit root and
round-trips a 4-byte payload. A second test pins project-only/host-grant
refusal (32 host tests).

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                488 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        32 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No TLS server/mTLS, no ALPN, no OCSP/CRL, no client certificates, no CLI
private-CA flag and no session resumption.
