# RFC-CM CM-A09 evidence: C ABI ResourceProvider v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `12be02e` plus this step's changes.
Decision record: [ADR-0036](../adr/0036-c-abi-resource-provider.md)
Header: [`marqdo_resource_provider.h`](../../crates/marqdo-host/include/marqdo_resource_provider.h)
Related: [in-process provider](2026-11-20-provider-abi-cm-a09.md),
[worker provider](2026-11-20-worker-provider.md), [STATUS §5](../STATUS.md).
Scope: trusted native C ABI adapter; plugin-Worker binding still open.

## ABI

```c
marqdo_resource_provider_abi_version() -> uint32_t (must be 1)
marqdo_resource_open(kind, descriptor, rights_csv, *token) -> i32
marqdo_resource_read(token, buffer, capacity, *read, *eof) -> i32
marqdo_resource_write(token, bytes, length, *written) -> i32
marqdo_resource_close(token) -> i32
marqdo_resource_last_error(buffer, capacity) -> i32
```

The host resolves these symbols at registration, rejects non-v1 libraries and
missing symbols, attaches callsite spans to errors, and never lets a provider
token enter a language `Value`, wire frame or trace.

## Test

`crates/marqdo-host/tests/c_provider.rs` (`#[cfg(unix)]`):

1. builds a C shared library with `cc -shared -fPIC` (skips if `cc` is absent);
2. registers it as provider `cabi` for kind `cabi.sensor` with a project-mode
   grant;
3. `resource.open/read/write/close` round-trips `c-`/`abi`, `write=3`, second
   close `resource.closed`;
4. a missing library path fails with `kernel.unavailable`.

No Rust runtime/`HostOp`/parser change was needed; language code still sees the
same `resource.*` surface.

## Security boundary

The C library is loaded in-process with host privileges. It is trusted native
code and explicitly **not** a sandbox. Pure/strict refuse provider kinds; strict
remains fail-closed. Untrusted plugin loading remains the separate plugin Worker
transport.

## Not done

* Plugin-Worker resource-provider binding through the existing plugin ABI.
* Provider package installation/signatures/discovery.
* ABI version negotiation beyond v1 rejection.
* Untrusted-provider isolation or strict-mode providers.
* Windows shared-library build/lane.
