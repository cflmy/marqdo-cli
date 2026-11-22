# ADR-0061: Detach the domain math kernel from the core

Status: Accepted (post-`3.0.0-beta.1`, RFC-EXT-0003 H-01/H-10/H-11).
Related: RFC-LANG-0002, ADR-0025, ADR-0032, ADR-0052, ADR-0062.
This ADR is an intentional **prerelease incompatibility**; it does not promise
zero-cost migration for `3.0.0-beta.1` users.

## Context

The core exposed five domain functions as first-class builtins:

- `math.parse`, `math.eval`, `math.substitute` over an exact-rational subset;
- `math.simplify`, `math.differentiate` declared but returning
  `math.unsupported`.

They required a core crate (`crates/marqdo-math`) in the host dependency graph,
reserved the `math` import namespace forever, made the `HostOp` enum grow with a
domain, and were not a complete CAS. Every new math algorithm would either grow
the core or be blocked by the reserved namespace. The beta audit (H-01, T-MATH-05/06)
recorded this as unintended domain ownership.

## Decision

1. **Remove the API from the core surface.** Delete the five `Builtin` entries,
   the five `HostOp` variants, their dispatch/runtime arms, the reserved
   `math` namespace as a builtin, and the `marqdo-math` workspace crate. A call
   to `math.parse/eval/substitute/simplify/differentiate` is a static
   `symbol.unknown`; there is no `math.unsupported` runtime fallback and no old
   compatibility parser.
2. **Keep only language primitives.** `Int`/`Float` literals, ordinary
   arithmetic/comparison/control flow, and the inert Markdown `Formula` value
   (a `$$...$$` block referenced with `ref("#anchor")`) stay in the core. The
   core never evaluates a `Formula`.
3. **Algorithms move out.**
   - Preferred: a pure `.mq.md` package exposing ordinary Exec H2 functions
     (`examples/math-library/` shows gcd + a consumer importing it as `math` now
     that the namespace is released).
   - Hot path / external data: an explicitly selected Plugin ABI v1 backend that
     requests the `math.compute` capability (`examples/math-backend/`). Selection
     is not authorization: native needs `--trust-native-plugin` and every plugin
     needs `--allow-plugin NAME`; without the grant it is `permission.denied`.
4. **`math.compute` stays.** `CORE_CAPABILITIES` may continue to name
   `math.compute` as a capability **category** requested by an ABI compute
   backend. It is not a promise that the core ships a solver, parser or evaluator,
   and pure language arithmetic never requires it.
5. **One binding, no fallback.** `check`, `run` and `call` see the same static
   result for a removed name. New math functions require neither `BUILTINS`,
   `HostOp`, `Host`, `Value`, `resource_request` nor a CLI rebuild.

## Consequences

- Beta users of `math.*` must migrate; `CHANGELOG`/`spec/math.md` carry the
  removed-name list, the `symbol.unknown` behaviour and a pure-package example.
- A `math` import alias is accepted again because no builtin namespace reserves it;
  `ext.import_namespace_conflict` no longer fires for `math`.
- The host no longer links `marqdo-math`; `cargo tree -p marqdo-host`/`-p marqdo-cli`
  contain no math crate. An architecture gate
  (`architecture_boundaries.rs`) fails if the crate or `HostOp::Math*` returns.
- The exact-rational engine is deleted rather than kept as dead core code. External
  libraries own their numeric contracts; API and precision are their responsibility.
- No Analysis v2, Trace v2, Value wire or capability wire version changes: only a
  builtin/HostOp surface and one crate are removed, and the static error code is
  the existing `symbol.unknown`.
- No OS sandbox, performance or CAS claim is made.
