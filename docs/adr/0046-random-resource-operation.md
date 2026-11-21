# ADR-0046: Explicitly granted `random.bytes`

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041
Specs: `spec/security.md`, `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-random-product.md`

## Context

The standard library had no product entropy API. Reading `/dev/urandom` or
using an OS RNG inside a Rust builtin would create an unaudited side channel
and would make `pure` mode non-deterministic without an explicit grant.

## Decision

Add one project-mode operation:

```text
resource.request("random.bytes", [count]) -> {ok:true, value:List<Int>}
```

* `count` is an `Int` in `1..=65536`; other values are fatal type errors.
* A new `Capabilities::random: bool` grant is required (`--allow-random`).
  Un-granted calls are `permission.denied`; `pure`/`strict` refuse.
* Bytes come from the `getrandom` crate's OS entropy source; a source failure
  is a fatal `host.error`.
* The operation returns a `List<Int>` of 0–255 values. It does not expose a
  `Memory` handle or raw pointer.
* `stdlib/random.mq.md` exposes `字节(n)` and `整数(上限)`. `整数` combines
  eight bytes and takes a modulus; it is **not** claimed to be unbiased
  rejection sampling.
* No new builtin or `HostOp` is added.

## Consequences

* Entropy is opt-in per run and unavailable in pure/strict.
* The API is byte-list based, so callers can build their own distributions in
  `.mq.md`; a cryptographically audited RNG API is out of scope.
* A future unbiased integer helper or secure token API needs its own ADR.
