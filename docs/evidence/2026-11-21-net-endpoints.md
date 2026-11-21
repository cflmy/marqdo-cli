# Evidence: DNS, UDP and TCP listener endpoints

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `ee0c6cc80a2782c4e88de63195eaf41a27077dcf`
Repository: `marqdo-cli`
Decision record: [ADR-0047](../adr/0047-net-dns-udp-listener.md)

## Implemented

- `resource.open("tcp", "host:port", ["read","write"])`: exact host grant is
  checked **before** `ToSocketAddrs`; every resolved address is tried with a
  5 s connect timeout.
- `resource.open("udp", "host:port", ["read","write"])`: connected datagram
  socket from an ephemeral local port; `read` returns one datagram with
  `eof=false`; `write` sends one datagram.
- `resource.open("tcp-listener", "host:port", ["accept"])` plus
  `resource.accept(listener)`: non-blocking 5 s poll, returns a new TCP
  `Resource` in `{ok:true,value:Resource}` or `tcp.timeout`.
- All project-mode only; pure/strict refuse; no TLS.

## Tests

`crates/marqdo-host/tests/resource_streams.rs` (30 tests) covers TCP DNS
round-trip via `localhost`, UDP datagram round-trip, listener accept/read/write,
host-grant denial and project-only refusal.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                486 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        30 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No TLS/certificate validation, no server sockets outside project mode, no UDP
broadcast/multicast, no listener port discovery API, no connection pool and no
DNS pinning. TLS remains a separate ADR.
