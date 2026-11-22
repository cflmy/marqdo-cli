# Math: core value and removed domain API

Status: **the `math.*` builtin API was removed from the core after
`3.0.0-beta.1`** by [ADR-0061](../docs/adr/0061-detach-domain-math-kernel-from-core.md)
(RFC-EXT-0003 H-01). This page documents what the core keeps, what it removed,
and how consumers migrate. It is not a contract for any external `marqdo-math`
package; that package owns its own function names and behaviour.

## 1. What stays in the core

- `Int` and `Float` literals and ordinary `+ - * / // %` arithmetic, comparison
  and pure language control flow.
- A Markdown `$$...$$` block remains an inert `Formula` resource in Analysis
  v2 `resources[]`; the core never evaluates it.
- The `math.compute` capability name remains a **capability category** that an
  explicitly selected ABI plugin may request (ADR-0062, H-11). The core hosts no
  math solver, parser or evaluator behind it.

Every other math algorithm belongs to a pure `.mq.md` package (preferred) or an
explicitly selected Plugin ABI backend for a hot path.

## 2. Removed builtins

The following `Builtin` names, `HostOp` variants and runtime arms no longer exist:

| Removed builtin | Previous behaviour |
|---|---|
| `math.parse` | Parse a formula resource into a `Formula` value |
| `math.eval` | Evaluate a formula with bindings |
| `math.substitute` | Substitute bindings into a formula |
| `math.simplify` | Host simplification (returned `math.unsupported`) |
| `math.differentiate` | Host differentiation (returned `math.unsupported`) |

A document that still calls one of them now fails **statically** with
`symbol.unknown` at `check`/`run`/`call`; there is no silent fallback to the old
host kernel and no `math.unsupported` runtime branch. `ref("#公式")` still resolves
the inert `Formula` resource, but no core builtin consumes it for math.

## 3. Migration

Use explicit pure functions and ordinary arithmetic. A pure library can expose a
namespaced function:

```markdown
# @knowledge 数学算法

## @exec 最大公约数

`a = 甲`

`b = 乙`

`while b != 0`

`余数 = a % b`

`a = b`

`b = 余数`

`end`

`return a`
```

Consumer:

```yaml
---
import:
  math: "./math-library/index.mq.md"
---
```

```markdown
## @exec main

`return math.最大公约数(甲=48, 乙=18)`
```

This needs no core change, no ABI and no CLI rebuild. The `math` import alias
is available again because the historical `math.*` builtin namespace no longer
reserves it (`ext.import_namespace_conflict` no longer applies to `math`).

For a high-performance algorithm, add an explicitly selected Plugin ABI v1 export
(for example `math_backend.add`) with a static signature and require
`--trust-native-plugin` for in-process execution plus `--allow-plugin NAME` for the
grant; without the manifest the call is `symbol.unknown`, without the grant it is
`permission.denied`. See `examples/math-backend/`.

## 4. What must not come back

- No new domain function may be added to `BUILTINS`, `HostOp` or
  `Host::resource_request` to emulate the removed API (ADR-0062).
- No `Value` variant or Analysis resource kind is added for math.
- No compatibility parser for the old `math.*` spelling.
