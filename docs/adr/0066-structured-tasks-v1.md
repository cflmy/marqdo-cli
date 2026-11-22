# ADR-0066 — Structured Tasks and bounded Channels

Status: Proposed. Follow-up to RFC-GPL-0004 §§5, 7 and 10.

## Execution and authority

Use a shared immutable prepared program and the existing callable resolution.
Spawn initially accepts a prepared FunctionRef with positional input. Each task
has its own interpreter frames, MemoryStore and Host resources. All spawned
tasks belong to the run. Task identities are opaque and scoped to that run.
Only an owning task may join or cancel its child. Supervisors finish by
reaping all children, including when the parent itself fails.

A child gets the intersection of its parent's effective grants and a requested
subset. If the subset is omitted, the child starts with no optional effect
grants. Charge every task, including descendants, to shared run quotas for
live tasks, queued work, fuel, memory, messages and output. A child deadline
cannot exceed its parent's. Bound scheduling admission before starting any
effect. The scheduler needs a defined strategy for joins and blocking sends
with a saturated worker pool. Prove progress for nested spawn with a two-worker
limit before freezing the design.

Transfer bounded snapshots of ordinary values. At every nesting level reject
Memory, Resource, PluginHandle, task handles and channel endpoints unless a
separate typed endpoint-transfer rule is specified. FunctionRef is resolved
against the same immutable prepared program. Table, Code and Formula transfers
need size and depth accounting. Channel queues are bounded by count and bytes.
Send is initially nonblocking with a full or closed result. Receive waits
until a value, closure, deadline or cancellation. Specify channel endpoint
ownership during the prototype before choosing any public syntax.

## Join, cancellation and observations

Join returns a structured outcome for completion, ordinary child failure and
cancellation. A join wait timeout leaves the child joinable. Explicit cancel
propagates to descendants and wakes waits. Capability denials and budget
violations terminate the offending task and remain distinguishable from
ordinary application failures. No task can use Join to resume an unauthorized
effect.

Cancellation is cooperative at interpreter safe points. Bound or exclude Host
operations that cannot honor run teardown, including uncancellable native
calls. Where a worker can be interrupted, termination and reap have separate
deadlines and must account for descendants. Task and parent IDs belong to a
versioned observability contract. Ordering is defined per task, with explicit
causal links across spawn, message delivery and join.

## Before acceptance

Exercise CPU recursion, concurrent mock network calls and batch orchestration
with an independent failure, deadline and success. Test nested joins under
pool saturation, queue and budget exhaustion, resource cleanup on parent exit,
permission attenuation and rejection of nested live handles. Compare the
existing single-task behavior and performance on the same workloads. Freeze
public names, Value/Handle representation and schema versions only after this
prototype. Shared memory and async syntax need separate decisions.
