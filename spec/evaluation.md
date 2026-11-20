# Marqdo Evaluation Semantics v1

Status: **frozen for M0.5**. Changes to observable behaviour require an ADR.

## 1. Values

| Type | Meaning |
|---|---|
| `Unit` | no result; a function that falls through returns `Unit` |
| `None` | explicit empty value; distinct from `Unit` |
| `Bool` | `true` / `false`; the only type usable in conditions |
| `Int` | arbitrary precision integer (`num-bigint`) |
| `Float` | IEEE-754 binary64 |
| `Text` | Unicode string |
| `List` | ordered values |
| `Map` | string-keyed record with deterministic (sorted) key order |
| `Table` | header names + rows of values (from GFM tables) |
| `Formula` | inert parsed LaTeX + provenance; math operations are explicit |
| `Code` | inert code fence + provenance; execution is explicit |
| `FunctionRef` | reference to a named function in a module |
| `PluginHandle` | opaque host-owned plugin export handle |

There is no truthiness: `if`-like conditions accept `Bool` only. There are no
implicit string↔number conversions; conversion builtins must be called.

## 2. Numeric semantics

* `Int` arithmetic (`+ - * // % **`) is exact. `/` always produces `Float`.
* Mixed `Int`/`Float` arithmetic yields `Float`; the conversion of an `Int`
  larger than 2^53 may lose precision and this is documented, not silently
  exported as a rounded `Int`.
* Comparisons between `Int` and `Float` are **exact** (the float is treated as
  the exact binary rational it is); no lossy comparison is performed.
* `//` and `%` require `Int` operands. Floor division: `a // b` is the
  mathematical floor; `a % b == a - b * (a // b)` (sign follows divisor).
* Division/modulo by zero is `runtime.division_by_zero`.
* `+` also concatenates two `Text` values. No other operator is overloaded.
* `**` on `Int` uses exact exponentiation; a negative exponent is a type error
  (use `/` or `Float` operands for fractional results). Exponents above a
  configurable budget are `resource.exhausted`.
* Floats may be `NaN`/`±Infinity` only as results of operations; they are
  tagged explicitly on the wire. NaN compares unequal to everything.

## 3. Evaluation order and scopes

* Blocks execute in Markdown AST document order; executable inline nodes
  execute in source order within a paragraph.
* A function call evaluates its arguments in source order, exactly once each,
  then binds them to fresh parameter slots in an isolated frame. The callee
  cannot see the caller's locals (no dynamic scoping); imported bindings are
  read-only.
* Assignment binds/updates a local slot in the current frame. It never writes
  an outer frame. There is no module-level assignment in v1: assignment
  outside a function body is an error.
* `and`/`or` short-circuit and require `Bool` operands.
* `Return` (an `emph` node) unwinds through branch/loop bodies to the
  enclosing function frame.
* `Branch` checks conditions in AST order and executes the first `true` branch
  once. `Loop` re-evaluates its condition before each iteration.
* Imported modules only register definitions for the importing document; they
  have no initialization side effects and their `main` is never started.
* Function calls recurse; recursion depth is bounded by the stack budget.
  Mutual recursion is allowed.

## 4. Errors

All failures are structured. Compile-time problems produce diagnostics
(`markdown.invalid_construct`, `expr.parse`, `symbol.unknown`, …). Runtime
problems produce `RuntimeError { category, span, message, frames, cause }`
with a stable category code. Runtime errors stop the current function and
propagate with a call stack. A host/foreign/plugin failure never panics the
interpreter; it becomes a structured error.

## 5. Parameter inference (static, name based)

Parameters are discovered by definite-assignment data-flow analysis over the
function's control-flow graph, never from prose:

1. Build the CFG of the function body in AST order, with branch and loop
   edges. Loop bodies may execute zero times.
2. Compute the set of variables *definitely assigned* on every path from the
   function entry.
3. Every read of a variable that is not definitely assigned at that point and
   that has no assignment anywhere in the function is a **candidate
   parameter** (sorted by canonical name).
4. If a variable is assigned on some paths but read before that assignment on
   others, its identity is ambiguous: emit `symbol.uninitialized` (S-04)
   instead of inventing a parameter.
5. Exclude builtins, imported aliases and reserved names from inference.
6. A read that is not explained by a parameter, a local, a builtin or an
   import is `symbol.unknown` with a hint (typo protection).

Consequences: `**结果 = n * 阶乘(n=n-1)**` infers `n`; `**n = 3**` followed by
reads makes `n` a local. Inferred parameters are named arguments only; an
explicit signature paragraph allows positional arguments.

## 6. Resource budgets

Every run has explicit limits (all configurable, all reported as
`resource.exhausted`): evaluation fuel, call stack depth, total output bytes,
wall-clock time, and host process count. Loops consume fuel per iteration and
expressions per node so that a hostile document cannot hang the CLI. Strict
mode additionally enforces limits at the OS level where available; if the
platform cannot provide that, strict mode refuses to run (see `security.md`).

## 7. Determinism

For a fixed source, mode, capabilities and budget, evaluation is
deterministic. Stdout/stderr from host operations are separated: program
output goes to stdout, diagnostics/traces go to stderr, and `--json` output
never mixes with program output.

## 8. Builtins v1

Builtins are part of the language surface and are capability-checked at the
host boundary. Names are stable; adding one requires a spec update.

| Name | Signature | Notes |
|---|---|---|
| `打印` / `print` | `(x) -> Unit` | writes `Value::display()` + newline to stdout |
| `长度` / `len` | `(x) -> Int` | Text scalar count, List/Map/Table size |
| `类型` / `type` | `(x) -> Text` | value type name |
| `文本` / `str` | `(x) -> Text` | explicit conversion |
| `整数` / `int` | `(x) -> Int` | from Int/Float(if integral)/Text(decimal) |
| `小数` / `float` | `(x) -> Float` | from Int/Float/Text |
| `表` / `table` | `(header: List, rows: List) -> Table` | validates row widths |
| `ref` | `(target: Text) -> FunctionRef/Formula/Code` | resolves a document address |
| `math.parse/eval/simplify/differentiate` | see `spec/math.md` | P5 |
| `foreign.run/call` | see `spec/foreign.md` | P5, capability-gated |
| `fs.read/write` | see `spec/security.md` | capability-gated |
