# RFC-CM CM-A09 evidence: C ABI provider worker binding v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `313ac03` plus this step's changes.
Decision record: [ADR-0038](../adr/0038-c-abi-provider-worker-binding.md)
Related: [C ABI](2026-11-20-c-provider.md), [worker transport](2026-11-20-worker-provider.md).
Scope: trusted helper binding; not a sandbox and not the plugin-Worker ABI.

## Artifact

`marqdo-provider-worker <library>` loads a `c-abi-1` shared library and serves
the ADR-0035 framed-JSON protocol (ping/open/read/write/close/shutdown). The
host registers it through the existing
`StdHost::register_worker_resource_provider` API; no runtime/`HostOp`/parser
change was required.

## Tests

`crates/marqdo-provider-worker/tests/worker_binding.rs`:

```text
c_abi_provider_runs_inside_the_worker_helper
  cc -shared C provider; worker binary loaded;
  resource.open/read "worker"/"-abi" eof/write/close through the host.

worker_helper_without_c_provider_fails_closed
  missing library -> structured host error, no in-process fallback.
```

The C provider state lives in the child; the host only sees the opaque
`ResourceHandle` and `ResourceRead` bytes.

## Boundary

The worker is a trusted helper with host-user privileges. It isolates crashes
and keeps the C library out of the interpreter process, but it is **not** an OS
sandbox. Pure/strict still refuse provider kinds; strict remains fail-closed.

## Not done

* Binding through the existing plugin-Worker manifest/artifact ABI.
* Provider registry/signatures/install commands.
* Untrusted-provider isolation, strict providers, Windows worker packaging.
* ABI version negotiation beyond rejecting non-v1 libraries.
