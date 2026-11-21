# ADR-0054: One static callable binding, one runtime dispatch

- Status: Accepted (2026-11-21)
- Supersedes the resolution model of ADR-0009/ADR-0010 where they conflict
- Implements: RFC-EXT-0001 §4 (PR-01/PR-02)

## Context

The tree had four independent ways to decide what a call means:

1. `marqdo-frontend/src/calls.rs` checked a Member call's builtin name before
   its import alias;
2. `marqdo-runtime/src/interpreter.rs` checked the import alias before the
   builtin for the same expression;
3. the runtime treated an undeclared `alias.name` as a plugin invocation when
   `program.external_callables` was empty ("try it at run time");
4. `marqdo-cli` synthesised an empty-parameter descriptor (`params: Vec::new()`)
   for exports that were loaded but not statically described.

Consequences observed while building RFC-EXT-0001's fixtures:

* `check` could accept a call that `run` dispatched to a different
  implementation (or vice versa);
* an unknown symbol failed only at run time, with a Host side effect already
  attempted;
* an import alias that happened to equal a builtin dotted namespace (`math`)
  silently changed the meaning of `math.eval(...)` depending on the pipeline;
* plugin arity/name errors could pass static checking and only fail (or
  succeed with different arguments) at the ABI boundary.

The language contract (RFC-LANG-0002) already says calls are expressions and
that `check` and `run` must agree; the implementation had no such guarantee.

## Decision

1. **One registry.** `marqdo_ir::callable::CallableRegistry` is the only static
   table. It contains builtins (including official bundled-stdlib public
   aliases), module functions keyed by canonical name plus unique local name,
   and exact plugin exports (`namespace.function`). Duplicate declarations are
   rejected when the registry is built; order never decides the answer.
2. **One binding pass.** `marqdo_frontend::calls::bind_program_calls(program,
   registry)` walks every statically visible call expression exactly once and
   produces `CallBindingTable` plus diagnostics. It replaces the split
   `validate_module`/`validate_program` priority logic on the CLI path (the
   standalone frontend keeps a wrapper for single-document tests). The result
   is installed on `Program::call_bindings`.
3. **Deterministic identity.** A call site is
   `(module id, function id, byte span)` (`CallSiteKey`), never a disk path,
   registration order or hash-map iteration. A bound target is one of:
   * `Builtin { canonical_name }` — fixed core function;
   * `Marqdo { function }` — exact `FunctionId` (module functions and official
     stdlib aliases such as `打印`/`print` → `io.mq.md::写值`);
   * `AbiPlugin { package, export, digest, param_names }` — complete export
     name plus the statically verified artifact digest and manifest argument
     order;
   * `DynamicValue` — a language value (for example a `FunctionRef`); this is
     the **only** non-static path and cannot reach an ABI export by name.
4. **Runtime dispatches by identity.** `eval_call` looks up the site's binding
   and calls the recorded target. It never calls `builtins::find(qualified)`,
   never inspects `external_callables.is_empty()` and never guesses a plugin.
   A missing binding is an internal error (`internal.error`), not a reason to
   re-resolve.
5. **Prepare finalises once.** `marqdo-cli` has one composition step
   (`project_prepare::prepare`): load sources → read extension descriptors
   (no `dlopen`/spawn) → build the registry → bind calls once. `check`,
   `run`/`call`, `inspect` and Analysis consume that one prepared program.
   Static plugin descriptors are inserted before binding, not appended after a
   first validation.
6. **Exact run-time reconciliation.** `run` verifies each selected plugin
   artifact, loads it, then compares the Host's actual exports (name, raw
   signature, artifact digest, owner package) with the static descriptors
   one-to-one. Any extra, missing or altered export fails with
   `ext.plugin_binding_mismatch` before the entry function runs. No descriptor
   is synthesised at run time.
7. **Print is not special.** `Program.print_override` and the runtime
   `打印`/`print` branch are removed. The builtin placeholder remains only for
   embedders without the official bundle and fails with
   `kernel.unavailable`; the official bundle declares aliases in verified
   metadata (ADR-0058).

## Priority matrix (normative)

See `spec/callable-resolution.md` for the complete matrix and diagnostics. In
short:

* `f(...)`: local variable → official alias/reserved builtin → current
  namespace → canonical name → unique local name → otherwise
  `symbol.unknown` (ambiguity is an error, never a first-match);
* `alias.f(...)`: explicit import alias → builtin dotted namespace → exact
  plugin export → otherwise `symbol.unknown`;
* `some_value.f(...)`: dynamic language dispatch only.

Import aliases that collide with a builtin or plugin dotted namespace are
rejected at prepare with `ext.import_namespace_conflict`; duplicate plugin
exports are rejected with `ext.duplicate_callable`.

## Consequences

* `check` and `run` now share binding identity; CALL-01…CALL-16 in
  `crates/cli/tests/callable_resolution.rs` pin the matrix.
* 0.x breaking change: a project importing a module under an alias that equals
  a builtin namespace (for example `math`) must rename the alias. There is no
  `--legacy` switch and no dual semantics.
* Analysis v2's `calls[].state/target` are now projected from the same binding
  (official aliases appear as `function` targets). Analysis v2 schema and
  fields are otherwise unchanged; private `CallBindingTable` is not serialized.
* Runtime no longer performs per-call name resolution, so the dynamic path is
  limited to language values. A text or map value called as a function is
  `call.not_callable` and never becomes a Host plugin request.
* Plugin manifests must be complete before binding: a manifest with a missing
  or malformed signature is a prepare error; ABI v1 default values are
  rejected until the wire semantics are versioned.

## Rejected alternatives

* **Pick a fixed precedence and keep two implementations.** Rejected: the RFC
  requires one static answer, and duplicated precedence is exactly the defect.
* **Build the registry lazily in the runtime.** Rejected: `check` would still
  not prove the binding, and runtime loading would change static resolution.
* **Keep the empty-`external_callables` fallback for "developer convenience".**
  Rejected: it turns a compile error into a Host call attempt and breaks
  `check`'s no-execution guarantee.
* **Expose `CallableId`/`CallBindingTable` in Analysis v2.** Rejected:
  private IR must not become a public wire contract without a version bump.
EOF
