# ADR-0038: C ABI provider worker binding v1

Status: **Accepted (trusted helper; existing plugin-Worker ABI binding still open)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A11, P5), ADR-0035, ADR-0036
Bin: `crates/marqdo-provider-worker`
Evidence: `docs/evidence/2026-11-20-c-worker-binding.md`

## Context

ADR-0036 loads a trusted C ABI provider in the host process; ADR-0035 provides a
framed-JSON worker transport. CM-A09 also asks for the Worker side of the C ABI:
the provider library should be loadable in a separate process so a crash does
not take down the interpreter, without inventing a new language surface. This
ADR adds the concrete binding.

## Decision

### 1. Worker binary

`marqdo-provider-worker <c-abi-library>`:

* loads exactly one `c-abi-1` shared library (ADR-0036 symbols);
* verifies `marqdo_resource_provider_abi_version() == 1` at startup;
* serves the ADR-0035 framed-JSON protocol (`ping/open/read/write/close/
  shutdown`) over stdin/stdout;
* maps C provider failures to `{"ok":false,"code":"host.error","message":...}`
  and never falls back to in-process loading.

The binary is a separate artifact, not a new top-level `marqdo` CLI command.
Registration is still host-side:
`StdHost::register_worker_resource_provider(name, kinds, worker_bin, [library])`
from ADR-0035.

### 2. Process boundary

* The C library runs in the worker process; a crash/exit is a structured host
  error and the host does not retry in-process.
* The worker has the host user's OS privileges; this is a **trusted transport**,
  not a sandbox and not proof of untrusted-provider safety.
* The language still sees only `resource.*`; provider tokens stay inside the
  worker and the host `ResourceHandle` remains the language value.
* Project mode + `allow_resource_provider(name)` are still required; pure/strict
  refuse provider kinds (CM-A11).

### 3. What remains for CM-A09

* Binding the provider contract through the **existing plugin-Worker ABI**
  (manifest/artifact ownership and version negotiation) rather than the
  dedicated helper.
* Provider registry/download/signatures and package installation commands.
* Untrusted-provider isolation and strict-mode providers.
* ABI version negotiation beyond v1.

## Consequences

* A trusted C adapter can be crash-isolated without changing the runtime,
  parser, `Value` or `HostOp`.
* The worker helper is small and reusable by a future provider package manager.
* The trust model is explicit: worker isolation ≠ sandbox; the provider library
  remains trusted code.

## Verification

`docs/evidence/2026-11-20-c-worker-binding.md` records a compiled C provider,
the worker helper, a full open/read/write/close round-trip through the host
worker transport, and fail-closed startup with a missing library.
