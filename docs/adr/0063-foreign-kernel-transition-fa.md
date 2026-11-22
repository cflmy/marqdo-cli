# ADR-0063: Foreign kernel transition — retain F-A, defer F-B

Status: Accepted (RFC-EXT-0003 H-02/H-12, post-`3.0.0-beta.1`).
Related: ADR-0020, ADR-0044, ADR-0062, [`spec/foreign.md`](../../spec/foreign.md).

## Context

The core exposes permission-gated one-shot `foreign.run`/`foreign.call` plus a
`KernelRegistry` whose default descriptors are Python/Node but which already
accepts dynamically registered languages. The audit (H-02) observed that:
`foreign.*` is a controlled **OS process boundary**, not a domain function; the
high-level batching/validation/orchestration around it should be a `.mq.md` package;
and removing the primitive before a generic process/stream capability exists would
push OS behaviour into an unregulated worker without improving safety.

Two legitimate but conflicting goals: "the frontend/runtime should not know each
external language API" (already true — registration is dynamic) versus "Foreign must
disappear from the core to count as decoupled" (not automatically true — some layer
must own the real OS process/IO boundary).

## Decision

**F-A (current, retained):** keep the existing permission-gated one-shot
`foreign.run`/`foreign.call` and the dynamic `KernelRegistry` as the controlled
OS-boundary primitive.

- No new language-specific `Builtin`, `HostOp` or runtime branch may be added.
  New languages register through `KernelRegistry::register`; Python/Node are only
  default descriptors, not the only extension channel.
- High-level orchestration (sequencing, batching, validation, error normalization)
  belongs in `.mq.md` packages, not in the host.
- Grants, digest/target checks, timeout, output caps, typed wire and environment
  binding stay exactly as `spec/foreign.md` specifies; the security negatives are
  not weakened by this ADR. Without `--allow-foreign` the call is
  `permission.denied`; `pure`/`strict` fail closed.

**F-B (deferred, needs its own ADR + release):** removing `foreign.*` from the
core requires first a versioned generic Process/Worker capability contract with at
least: program/argv, controlled stdio, isolation level, timeout and process-tree
reaping, output budget, environment identity, lifecycle and cancellation
semantics. It must demonstrate frozen-binary behavioural equivalence and keep the
same permission, digest, timeout, wire and output-limit negatives. Until then the
core keeps the primitive and we do not claim it was removed.

**Forbidden in both phases:** persistent Python/Node sessions, Notebook-cell
execution, `@python`-style heading auto-execution, and a second foreign runtime
inside View; streaming stays in Execution Events, not a second network protocol.

## Consequences

- The core is honest: it owns a controlled foreign primitive and documents it.
- Adding a new language needs zero Rust/core changes (T-FOREIGN-01 remains the
  acceptance test); the default descriptor table may later become a verified static
  config, but that is not required to decouple domains.
- F-B stays open and tracked; docs/STATUS records it as deferred rather than as
  implemented.
