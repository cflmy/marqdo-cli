# ADR-0042: `clock.now` resource operation

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-datetime-clock-product.md`

## Context

The bundled standard library needs a product clock API. Reading the OS clock
inside a Rust builtin would add a second side-effect path beside
`resource.request`, and making time available in every mode would break the
pure/strict fail-closed contract without an explicit capability decision.

## Decision

Extend `resource.request` with one non-filesystem operation:

```text
resource.request("clock.now", []) -> {ok:true, value:UnixMillis}
```

* Zero arguments; any argument is a fatal `call.arity` error.
* Project mode only. `pure` and `strict` return `permission.denied`, like
  `fs.*`.
* The value is UTC Unix milliseconds, clamped to `i64`. A clock before the
  Unix epoch is a fatal `host.error`.
* The call records the same `resource.request` semantic/effect events with
  `op=clock.now`.
* `stdlib/clock.mq.md` is a thin `.mq.md` facade; `stdlib/datetime.mq.md` is a
  pure UTC formatter/parser over Unix milliseconds and has no host dependency.

## Consequences

* The clock remains a capability-gated host operation, not a language keyword.
* No new CLI flag is introduced; project mode already implies the existing
  host boundary. A future per-call clock grant can replace this default.
* Monotonic time, time zones and locale formatting are not part of this
  decision.
