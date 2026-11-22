# ADR-0064 — Stable core and extension boundary

Status: Accepted for the post-beta development line. Input is [RFC-GPL-0004,
issue #4](https://git.cflmy.cn/cflmy/marqdo-cli/issues/4).

## Decision

The frontend owns the single Markdown-to-IR mapping and static callable binding.
The runtime owns evaluation and run-local state. The Capability Host owns
authority, resource identity, budgets and observable effects. Tool v1 dispatches
independent processes. Domain code goes in a Marqdo package, a Callable Plugin,
a Resource Provider, a Foreign Kernel, or a Tool, according to which existing
interface it needs.

Keep the core surface inventory under review. A change to the builtin catalogue,
HostOp, Value or top-level command set requires a new accepted ADR, the relevant
specification changes, and evidence from at least three unrelated use cases
that need the same primitive. Include a working composition example or explain
why the existing interfaces cannot express one. Describe authority, ownership,
cleanup, performance, compatibility, and positive and negative tests.
The reviewed resource operations remain governed by ADR-0062 and Foreign F-A
by ADR-0063.

The Linux check builds the release CLI and its shipped provider worker, installs
the archive, and runs a pinned external conformance repository against it.
The evidence identifies the core and fixture commits and the executable
digests. New extension use cases should enter that repository first. The
stock package, Worker Callable, Foreign Kernel, Resource Provider, and
registered Tool cases continue to exercise the same prebuilt CLI.

Feature work that depends on new runtime primitives follows
[Resource Control](0065-resource-control-vnext.md) and
[Task v1](0066-structured-tasks-v1.md) as independent milestones. Each proposal
has its own adoption and acceptance criteria. HTTP and model-specific behavior
belongs to independent reference packages using the published interfaces.

## Consequences

Extension packages depend on public contracts. The source and ABI for those
contracts keep their existing release policy. The inventory gate covers changes
to the named surfaces and the external tests cover composition at runtime.
Review still checks changes inside an existing dispatch branch.
