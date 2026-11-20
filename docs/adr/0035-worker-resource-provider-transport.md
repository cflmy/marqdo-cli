# ADR-0035: Worker resource-provider transport v1

Status: **Accepted (trusted helper transport; C ABI binding still open)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A11, P5), ADR-0033, ADR-0034
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-worker-provider.md`

## Context

ADR-0033 provides an in-process `ResourceProvider` v1. CM-A09 also asks for a
process boundary so an adapter can crash or hang without taking down the
interpreter, and for a transport that can later be implemented by the C/Worker
plugin ecosystem. This ADR adds the first process-boundary transport: a
host-managed helper process speaking framed JSON.

It deliberately does **not** claim the C ABI or OS sandbox: the helper runs as
the host user and is trusted. The existing plugin Worker transport remains the
untrusted-plugin path; this is a resource-provider transport for explicitly
trusted adapters.

## Decision

### 1. Framing and protocol v1

* 4-byte big-endian length + JSON, maximum 1 MiB per frame.
* A `ping` handshake must succeed before the provider is registered.
* Operations:

```text
{"op":"open","kind","descriptor","rights"} -> {"ok":true,"token":u64}
{"op":"read","token","max_bytes"}          -> {"ok":true,"bytes":[u8...],"eof":bool}
{"op":"write","token","bytes":[u8...]}     -> {"ok":true,"count":u64}
{"op":"close","token"}                     -> {"ok":true}
{"op":"shutdown"}                          -> process exit
{"ok":false,"code","message"}              -> structured failure
```

* Provider tokens stay in the helper; the host still issues the opaque language
  `ResourceHandle`. Bytes are copied per call.
* The host maps helper error codes to the existing categories
  (`permission.denied`, `resource.closed`, `resource.exhausted`,
  `resource.unsupported`, otherwise `host.error`) and attaches the callsite
  span.
* The provider is killed on drop; a protocol/IO failure is `host.error`, never a
  fallback to another transport.

### 2. Registration and authorization

* `StdHost::register_worker_resource_provider(name, kinds, program, args)` is a
  host embedding API. It validates name/kinds with the same rules as ADR-0033
  (non-empty, not `stdio`/`file`, no duplicate kind), then registers the
  provider under its kinds.
* `resource.open` still requires the provider name grant and project mode.
  Pure/strict refuse provider kinds; strict remains fail-closed.
* Language code cannot spawn a helper; registration is not reachable from
  `.mq.md` or a package import.

### 3. Security boundary

* The helper runs as the host user with full host privileges. It is **trusted**;
  this transport is not a sandbox and does not make arbitrary native code safe.
* The project-mode + name-grant checks are the same as ADR-0033. The helper's
  own descriptor/rights validation is additional, not a substitute.
* No network, filesystem or process rights are delegated implicitly by
  registration; the helper is responsible for its own behavior, and the host
  grant only authorizes the resource kind.

### 4. What remains for CM-A09

* A C ABI / plugin-Worker binding for `ResourceProvider` (buffer handoff,
  ownership, version negotiation through the plugin ABI).
* Provider package installation, signatures and discovery.
* Untrusted-provider isolation and strict-mode provider support.
* The transport is versioned (`protocol_version() == 1`); future versions need a
  new ADR and negotiation.

## Consequences

* A trusted adapter can live in its own process and be reached through the same
  `resource.*`/`Result` language surface without touching the parser, `Value`,
  runtime dispatch or `HostOp`.
* A helper crash/exit is a structured host error; the interpreter never falls
  back to a different provider.
* The helper protocol is small enough to reimplement in C/Worker adapters later.

## Verification

`docs/evidence/2026-11-20-worker-provider.md` records a Python helper exercising
ping/open/read/write/close, grant/pure/strict denial, missing-program error and
the full workspace gate.
