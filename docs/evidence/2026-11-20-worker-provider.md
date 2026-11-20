# RFC-CM CM-A09 evidence: worker resource-provider transport v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `12be02e` plus this step's changes.
Decision record: [ADR-0035](../adr/0035-worker-resource-provider-transport.md)
Related: [in-process provider](2026-11-20-provider-abi-cm-a09.md),
[STATUS §5](../STATUS.md).
Scope: trusted host-managed helper process; not a sandbox and not the C ABI binding.

## Transport

`crates/marqdo-host/src/provider_worker.rs` implements the ADR-0035 framed-JSON
transport:

```text
4-byte BE length + JSON, max 1 MiB
ping handshake
open  -> token
read  -> bytes+eof
write -> count
close
shutdown on drop
```

`StdHost::register_worker_resource_provider(name, kinds, program, args)` spawns
the helper and registers it through the same validation/grant path as ADR-0033.
The helper token never enters a language `Value`; the host still issues the
opaque `ResourceHandle`.

## Tests

`crates/marqdo-host/tests/worker_provider.rs` (3 tests):

```text
worker_provider_round_trips_through_framed_json
  Python helper: ping/open/read 5 "hello"/read rest " world" eof/write 3/close;
  second close resource.closed.

worker_provider_registration_requires_a_grant_and_project_mode
  no name grant -> permission.denied; pure/strict -> permission.denied.

missing_worker_program_is_a_structured_error
  non-existent helper -> kernel.unavailable (no fallback).
```

No runtime, `HostOp`, `Value` or parser change was needed; language code still
uses `resource.*`.

## Security boundary

The helper runs as the host user with host privileges. Registration is an
embedding API, project-mode only, and requires a provider-name grant. It is
trusted infrastructure, **not** an OS sandbox; the untrusted-plugin path remains
the separate plugin Worker transport.

## Not done

* C ABI / plugin-Worker binding for `ResourceProvider`.
* Provider package installation/signatures/discovery.
* Untrusted-provider isolation; strict-mode provider support.
* External DNS/TLS/network endpoint work beyond the IP-literal TCP slice.
