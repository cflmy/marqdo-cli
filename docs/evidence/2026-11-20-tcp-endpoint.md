# RFC-CM P5 evidence: blocking TCP resource endpoint v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `1d87f98` plus this step's changes.
Decision record: [ADR-0034](../adr/0034-tcp-resource-endpoint.md)
Related: [provider v1](2026-11-20-provider-abi-cm-a09.md), [STATUS §5](../STATUS.md).
Scope: IP-literal blocking TCP only; no DNS/TLS/UDP/server sockets.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  426 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Contract

```text
resource.open("tcp", "<ip-literal>:<port>", ["read", "write"])
```

* project mode only; `pure`/`strict` refuse;
* exact IP-literal grant (`Capabilities.check_net`), CLI `--allow-net <IP>`;
* DNS names rejected with `resource.unsupported` (no resolver is called);
* connect/read/write timeouts 5 seconds; blocking I/O on the interpreter thread;
* `resource.read` returns `{count, eof}`, `write` a possibly short count,
  `close` shuts the socket down; closed/forged handles use existing rules;
* trace records `resource.open/read/write/close` effects (IP:port on open, no
  payload bytes).

No new `HostOp`, `Value`, builtin or Markdown syntax.

## Tests

Host (`crates/marqdo-host/tests/resource_streams.rs`, 2 TCP tests):

```text
tcp_resource_round_trips_with_a_network_grant
  loopback echo: write ping -> read ping -> close

tcp_requires_network_grant_and_rejects_dns_names
  no --allow-net -> permission.denied
  "example.test:80" -> resource.unsupported
  pure/strict -> permission.denied
```

CLI (`crates/cli/tests/resource_streams.rs`):

```text
tcp_stream_round_trips_through_mqmd_with_a_network_grant
  local TcpListener; .mq.md writes/reads 4 bytes through Memory;
  run --allow-net 127.0.0.1 --json -> [4,4,112,105,110,103,true]
```

## Not done

* DNS, TLS, HTTP framing, proxies, UDP, raw sockets, socket options.
* Nonblocking/async/select and server bind/listen.
* Strict-mode TCP and any claim that `std::net` in-process is OS isolation.
* Provider package installation/registry for network endpoints.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-host --test resource_streams tcp
cargo test -p marqdo-cli --test resource_streams tcp
cargo test --workspace --locked
```
