# Marqdo callable resolution and binding

Status: **normative for the current implementation** (ADR-0054/ADR-0058,
RFC-EXT-0001 §4). This document describes the single static resolution used by
`check`, `run`, `call`, `inspect` and Analysis. There is no second priority
order and no runtime name guessing.

## 1. Identity

A call is an expression (`Expr::Call`). Each statically visible call site has a
deterministic key:

```text
CallSiteKey = (module id, function id, span.start, span.end)
```

The key is independent of the checkout path, package cache location,
registration order and map iteration. For a signature-default expression the
owning function is the function that declares the default, because the runtime
evaluates defaults in that function's empty frame.

A binding is one of:

| Bound target | Meaning |
|---|---|
| `Builtin { canonical_name }` | fixed language builtin, addressed by its canonical name (never by a user alias) |
| `Marqdo { function }` | exact `FunctionId` of a Markdown-defined function, including an official bundled-stdlib public alias |
| `AbiPlugin { package, export, digest, param_names }` | exact plugin export; `digest` is the selected, verified artifact SHA-256 and `param_names` is the manifest order used to normalise positional arguments |
| `DynamicValue` | the callee is a language value (for example a `FunctionRef`); resolved at run time from the value, never by name |

`CallBindingTable` is private IR. It is not serialized into Analysis or Trace.
The public `calls[].state/target` projection is derived from it (see §6).

## 2. Resolution matrix

### 2.1 Unqualified `f(...)`

In this exact order; the first applicable step decides:

1. **Local variable.** If `f` is a parameter or has been assigned anywhere in
   the function body, the site binds `DynamicValue`. At run time the value must
   be callable (`FunctionRef` in this release); a `Text`/`Map`/etc. value is
   `call.not_callable`. This is a language variable rule, not an escape hatch to
   a plugin.
2. **Official alias or reserved builtin.** The registry's builtin table contains
   fixed builtins and, when the verified bundle is present and the program uses
   them, official stdlib aliases. An official alias wins over the placeholder
   builtin of the same name. Third parties cannot insert names here.
3. **Current namespace function.** If the enclosing level-1 namespace `N`
   declares `N.f`, that function is chosen.
4. **Canonical function name.** A function whose canonical name is exactly `f`.
5. **Unique local name.** A function whose `local_name` is `f` in exactly one
   namespace. If two namespaces define the same local name and none of steps
   1–4 applies, the call is an error (`symbol.unknown`, message reports the
   ambiguity); it is never "first in document order".
6. Otherwise `symbol.unknown`.

### 2.2 Qualified `alias.f(...)`

1. **Local variable named `alias`.** If `alias` is a local value, the site binds
   `DynamicValue`; `alias.f` is ordinary language member access. This cannot
   become a plugin call.
2. **Explicit import.** If the module declares `import: { alias: <target> }`,
   the call binds only a function of that imported module (canonical name first,
   then unique local name). A missing function is `symbol.unknown`; it never
   falls back to a builtin or plugin with the same dotted spelling.
3. **Builtin dotted namespace.** Otherwise, an exact builtin
   `alias.f` (for example `resource.request`) binds `Builtin`.
4. **Exact plugin export.** Otherwise, if a selected static plugin declares the
   complete name `alias.f`, the site binds that export. The manifest signature
   is validated here; no library is loaded.
5. Otherwise `symbol.unknown`.

`alias` in step 1/2 must be exactly an explicit import or a local value; there
is no implicit namespace search across packages.

### 2.3 Other callees

`some_value.f(...)`, `f_list[i](...)`, `(expr)(...)` and any other non-Ident,
non-member-of-Ident callee bind `DynamicValue`. Only language callables can be
invoked at run time.

### 2.4 Import namespace conflicts

Before any call is bound, each module's import aliases are checked against:

* builtin dotted namespaces (`alias.` must not prefix any builtin name), and
* visible plugin dotted namespaces (`alias.` must not prefix any selected
  plugin export).

A collision is `ext.import_namespace_conflict` at the front-matter import span,
with a rename hint. Example: importing `./math.mq.md` as `math` conflicts with
the builtin namespace `math.`; rename the alias to `mymath`. This is a
deliberate 0.x breaking change with no compatibility switch (ADR-0054).

### 2.5 Duplicate callables

The static table rejects:

* the same complete plugin export declared by two selected packages
  (`ext.duplicate_callable` at prepare);
* a plugin export that equals a builtin name or an official alias
  (`ext.duplicate_callable`);
* two different manifests for the same plugin package identity in one selection
  (`ext.duplicate_callable`); the exact same package identity is idempotent;
* a malformed/ambiguous static table entry (`ext.duplicate_callable`).

Order of `--plugin` flags never decides which declaration wins.

## 3. Argument checking

### 3.1 Markdown functions and builtins

`CallableSignature::validate_args` enforces:

* named arguments must exist exactly once; unknown names are `call.arity`;
* positional arguments require an explicit language signature and map to the
  declared parameter order; overflow is `call.arity`;
* required parameters without defaults must be supplied; missing is
  `call.arity`;
* inferred signatures accept named arguments only (`call.arity` otherwise).

Messages are deterministic and identical between `check` and `run`.

### 3.2 ABI plugin exports

* The static manifest signature is authoritative. Unknown, duplicate, missing
  or excess arguments are rejected at prepare/bind time (`call.arity`); the
  plugin is never spawned for a statically invalid call.
* ABI v1 transports arguments as a named map. Positional arguments are
  statically validated and deterministically normalised to the manifest's
  declared parameter order at run time. Example: `solver.add(1, b=2)` becomes
  `{a:1,b:2}`.
* Manifest signature default values are rejected in ABI v1 until the wire
  semantics are versioned; no default is injected at run time.
* A plugin that is declared but not selected (`--plugin` missing) is
  `symbol.unknown`; a selected plugin without an exact export descriptor is
  `symbol.unknown`. There is no runtime fallback.

## 4. Run-time reconciliation

`run`/`call` performs, in order:

1. static prepare (sources, manifests, signatures, binding) — no binary loaded;
2. verify each selected artifact path, target and SHA-256;
3. verify the `--allow-plugin` grant;
4. load exactly the selected artifacts;
5. compare the Host's actual exports with the static descriptors one-to-one
   (`name`, raw signature, artifact digest, owner package).

A mismatch in step 5 is `ext.plugin_binding_mismatch`, reported before the
entry function runs. `run` must never synthesise a missing descriptor or an
empty signature.

## 5. `check` versus `run`

`check` and `run` consume the same `PreparedProgram` binding table. Therefore:

* the same call site has the same `BoundTarget` in both;
* a statically invalid call fails `check` and `run` with the same diagnostic
  code and message; `run` refuses before any Host side effect;
* `check --plugin` reads manifests and signatures only: it never `dlopen`s a
  library, spawns a worker or executes a side-effect script; a missing plugin
  binary is not an error for `check`;
* `--emit` Analysis is derived from the same binding (no second resolver).

## 6. Analysis projection

`calls[]` keeps the v2 fields `{function, span, callee, state, target}`.
`state` is one of `function`, `builtin`, `plugin`, `unresolved`; it is now
derived from the binding table:

| Bound target | `state` | `target` |
|---|---|---|
| `Builtin` | `builtin` | canonical builtin name |
| `Marqdo` | `function` | exact `FunctionId` string |
| `AbiPlugin` | `plugin` | complete export name |
| `DynamicValue` / unbound | `unresolved` | `null` |

Official stdlib aliases (for example `打印`) therefore project as `function`
targets, because that is the implementation that runs. This is a projection of
the existing fields, not a new public schema. The private `CallBindingTable` is
never exposed; a future explicit `resolved_call_target` field would require an
Analysis version bump.

## 7. Diagnostics

| Code | Stage | Trigger |
|---|---|---|
| `symbol.unknown` | prepare/bind | no target for the call; ambiguity; unknown imported function; undeclared plugin export |
| `call.arity` | prepare/bind | bad argument name/number/duplication, invalid positional use |
| `ext.import_namespace_conflict` | prepare | import alias collides with a builtin/plugin dotted namespace |
| `ext.duplicate_callable` | prepare | duplicate complete export, builtin shadowing, conflicting package manifests |
| `ext.plugin_binding_mismatch` | run init | loaded exports/signatures/owner/digest differ from static descriptors |
| `ext.no_matching_artifact` | manifest read | plugin/provider has no artifact for the current target |
| `ext.artifact_integrity` | manifest read / run init | sha256 mismatch, path traversal, non-regular file, symlink escape |
| `call.not_callable` | run | dynamic value is not callable |
| `internal.error` | run | a call site has no binding (a prepared program must always be complete) |

Existing codes are reused rather than duplicated; `ext.capability_handler_unavailable`
is internal-only — the public wire code remains `capability.unavailable`.

## 8. Examples

### 8.1 Imported function, no conflict

```markdown
---
import:
  numerics: "./math.mq.md"
---

## @exec main

`return numerics.最大公约数(甲=48, 乙=18)`
```

Binds `Marqdo { function: <package instance>::math.mq.md::最大公约数 }` in both
`check` and `run`. The historical `math.*` builtin namespace was removed
(ADR-0061), so a `math:` import alias no longer conflicts with a builtin.

### 8.2 Builtin without an import

```markdown
## @exec main

`return resource.request("clock.now", [])`
```

Binds `Builtin { canonical_name: "resource.request" }`.

### 8.3 Import namespace conflict

```markdown
---
import:
  resource: "./resource.mq.md"
---

## @exec main

`return resource.request("clock.now", [])`
```

`check` fails with `ext.import_namespace_conflict`; `run` refuses before
executing anything. Rename the alias. Removed builtins such as `math.eval` are no
longer reserved names: a conflicting alias is now impossible for that namespace.

### 8.4 Plugin export

```markdown
## @exec main

`return solver.add(1, b=2)`
```

* without `--plugin solver/plugin.yaml` or with a manifest that does not
  declare `solver.add`: `symbol.unknown` (no Host fallback);
* with the manifest: binds `AbiPlugin { package: "demo.solver", export:
  "solver.add", digest: "<sha256>", param_names: ["a","b"] }`;
* with a changed artifact byte: `ext.artifact_integrity` before the entry
  function;
* with a missing `b`: `call.arity` at prepare.

### 8.5 Dynamic `FunctionRef`

```markdown
## @exec 平方

`params(n: Int)`

`return n * n`

## @exec main

`f = ref("#平方")`

`return f(n=4)`
```

The call site binds `DynamicValue`; the runtime calls the `FunctionRef` value.
A `Text` value in the same position fails `call.not_callable` and never reaches
the Host.
