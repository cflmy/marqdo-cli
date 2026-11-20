# RFC-CM CM-A09 evidence: in-process ResourceProvider v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a8e62a5` plus this step's changes.
Decision record: [ADR-0033](../adr/0033-in-process-resource-provider-v1.md)
Related: [resource streams](2026-11-20-resource-stream-p3.md), [STATUS §5](../STATUS.md).
Scope: trusted in-process provider contract only; C/Worker transport remains open.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  423 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Contract

`ResourceProvider` v1:

```text
name() -> stable grant name
protocol_version() -> 1
kinds() -> resource kinds
open(kind, descriptor, rights) -> provider token
read(token, max_bytes) -> {bytes, eof}
write(token, bytes) -> count
close(token)
```

The host still issues the language `ResourceHandle`; the provider token never
enters a `Value`, wire message or trace. Bytes are copied for the call duration;
providers must not retain Memory pointers.

## Registration and authorization

* `StdHost::register_resource_provider` rejects protocol != 1, empty kinds,
  reserved `stdio`/`file` names/kinds and duplicate kind registration.
* `Capabilities.resource_providers` + `allow_resource_provider(name)` add the
  grant; `resource.open` requires the provider name to be granted.
* Project mode only. Pure and strict refuse provider kinds; registration grants
  nothing by itself.
* In-process providers are trusted and **not** sandboxed; the ADR states this
  explicitly.

## Tests

`crates/marqdo-host/tests/resource_streams.rs` (12 total; 3 provider tests):

```text
registered_provider_streams_work_with_a_grant_in_project_mode
  mock sensor: read ab/cdef with eof, write 1, close, second close resource.closed

provider_requires_a_name_grant_and_project_mode
  no grant -> permission.denied
  pure -> permission.denied
  strict -> permission.denied

provider_registration_rejects_versions_reserved_names_and_conflicts
  version 2, reserved stdio kind, duplicate kind -> resource.unsupported
```

No parser, `Value`, runtime dispatch or `HostOp` change was required; the mock
provider is registered through a host API and the language sees the existing
`resource.*` calls.

## Not done

* C ABI / Worker resource transport: buffer handoff, ownership, crash/timeout
  isolation and version negotiation through the plugin ABI.
* Provider package installation, signatures, discovery and platform matrix.
* Network/socket/TCP/HTTP, clock, random, process, directory, device providers.
* Strict-mode provider verification.
* No external `marqdo-stdlib` repository publication.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-host --test resource_streams
cargo test --workspace --locked
```
