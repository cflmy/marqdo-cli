# ADR-0036: C ABI ResourceProvider v1

Status: **Accepted (trusted native adapter; plugin-Worker binding still open)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A11, P5), ADR-0033, ADR-0035
Header: `crates/marqdo-host/include/marqdo_resource_provider.h`
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-c-provider.md`

## Context

ADR-0033 provides an in-process provider trait and ADR-0035 a host-managed
helper process with framed JSON. CM-A09 also needs a **C ABI** so native
adapters can expose resource endpoints without linking Rust or changing the
runtime. This ADR freezes the first C ABI version for trusted native adapters.

## Decision

### 1. Version and symbols

```c
#define MARQDO_RESOURCE_PROVIDER_ABI_VERSION 1u

uint32_t marqdo_resource_provider_abi_version(void);
int32_t  marqdo_resource_open(const char *kind, const char *descriptor,
                              const char *rights_csv, uint64_t *out_token);
int32_t  marqdo_resource_read(uint64_t token, uint8_t *buffer,
                              uint32_t capacity, uint32_t *out_read,
                              int32_t *out_eof);
int32_t  marqdo_resource_write(uint64_t token, const uint8_t *bytes,
                               uint32_t length, uint32_t *out_written);
int32_t  marqdo_resource_close(uint64_t token);
int32_t  marqdo_resource_last_error(char *buffer, uint32_t capacity);
```

* The host loads the library, requires ABI version 1 and resolves the symbols
  at registration; missing symbols/version mismatch fail closed with
  `resource.unsupported`/`kernel.unavailable`.
* Return `0` on success, nonzero on failure; failure text comes from
  `last_error` and is surfaced as a structured host error with the callsite
  span.
* `rights_csv` is a comma-separated list. Provider tokens are private to the
  adapter; the host still issues the language-visible `ResourceHandle`.
* Buffers are borrowed for the call only; the adapter must not retain host
  pointers. The host caps frame/buffer sizes by the existing resource limits.

### 2. Registration and authorization

* `StdHost::register_c_resource_provider(name, kinds, library_path)` is a host
  embedding API; it uses the same name/kind validation as ADR-0033 and the same
  provider-name grant at `resource.open`.
* Project mode only; pure/strict refuse provider kinds. Strict stays
  fail-closed (CM-A11).
* The C library runs in the host address space with host privileges. It is
  **trusted**, not sandboxed; the untrusted-plugin path remains the separate
  plugin Worker transport. Registration is not reachable from `.mq.md`.

### 3. Remaining CM-A09 work

* Plugin-Worker binding for `ResourceProvider` through the existing plugin ABI
  (buffer handoff/ownership/version negotiation).
* Provider package installation, signatures and discovery.
* Untrusted-provider isolation and strict-mode provider support.
* ABI version negotiation beyond rejecting non-v1 libraries (future ADR).

## Consequences

* Native adapters can expose new resource kinds through `resource.*` without
  parser/`Value`/runtime/`HostOp` changes.
* ABI mistakes are fail-closed at registration or surfaced as host errors; no
  fallback transport is attempted.
* The header is small and stable enough for a future SDK/package workflow.

## Verification

`docs/evidence/2026-11-20-c-provider.md` records a compiled C test provider
(`cc -shared`), version/load checks, open/read/write/close round-trip, missing
library fail-closed and the full workspace gate.
