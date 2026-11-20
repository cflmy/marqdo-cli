# ADR-0009: One callable resolver for builtins, Marqdo functions and ABI exports

Status: **Accepted**
Date: 2026-11-20

## Context

M0.5 resolves calls in three unrelated places:

* `crates/marqdo-ir/src/builtins.rs::BUILTINS` plus
  `crates/marqdo-frontend/src/calls.rs` for static validation;
* `crates/marqdo-runtime/src/builtins_impl.rs::dispatch` for builtin execution;
* `crates/marqdo-runtime/src/interpreter.rs` for Marqdo function lookup, with an
  ad-hoc fallback that sends any unknown `alias.name` member call to the host as
  a trusted plugin call.

Adding one external function therefore risks touching the builtin table, the
frontend, the runtime and the host. The frontend cannot statically validate a
plugin call because it has no descriptor for the export, and the runtime decides
by "first builtin/function/plugin that matches".

M0.6 R1/R2 require a single static description and a single runtime binding
path. The language expression grammar and control flow remain fixed; only the
symbol layer becomes registry-driven.

## Decision

### 1. Descriptor

`marqdo-ir::callable` defines:

```rust
pub struct CallableId(pub String); // canonical, globally unique

pub enum CallableOrigin {
    Builtin,
    Marqdo,   // a FunctionId
    AbiPlugin { package: String, export: String },
}

pub struct CallableDescriptor {
    pub id: CallableId,
    pub public_name: String,          // `print`, `数学.阶乘`, `solver.add`
    pub owner_package: PackageInstanceId,
    pub signature: CallableSignature,
    pub effects_requested: Vec<String>,
    pub origin: CallableOrigin,
    pub implementation_digest: Option<String>,
    pub source_span: Option<Span>,
}

pub struct CallableSignature {
    pub params: Vec<Param>,
    pub kind: SignatureKind,
    pub result: Option<String>,
}
```

Builtins keep their static `BUILTINS` table: they are part of the language, not
an extension point. The table is wrapped in descriptors; it is not copied into
every plugin.

### 2. Resolution

`CallableRegistry` is built deterministically from:

1. builtin descriptors (fixed, reserved names);
2. all program functions (canonical and unique local names);
3. explicitly declared external descriptors supplied by the CLI/package layer
   from plugin manifests (never by loading a dynamic library during `check`).

Rules:

* fully qualified names are unique; duplicate external names are a hard
  conflict, unless the caller explicitly aliases one (`solver.add as add`);
* an unknown function is a compile error, never "try builtins/plugin at
  runtime";
* `check` validates arity/parameter names from the static descriptor for every
  origin, including ABI exports;
* `run` binds the already-resolved `CallableId` and dispatches by origin;
* `Host` still performs capability authorization at the actual effect, based on
  the package/plugin identity and the current execution context.

The existing `BUILTINS` list remains a closed language set. Third-party
extensions never edit it, `calls.rs` match arms or
`builtins_impl.rs::dispatch`.

### 3. Plugin calls

A plugin descriptor is derived from `plugin.yaml` exports
(`name`, `signature`, package, digest). `check --plugin <manifest>` (or a
manifest declared in `marqdo.yaml`) may add these descriptors statically without
loading any shared library. The runtime uses the export's complete dotted name
(`solver.add`), not only the first prefix.

A call form `solver.add(a=1, b=2)` resolves to a `CallableDescriptor` whose
origin is `AbiPlugin`. If no descriptor was declared, it is `symbol.unknown`;
it is not sent to the host speculatively.

### 4. Effects

`effects_requested` is a conservative static summary used for reporting and
permission prompts. It is not proof that an implementation is pure. At
execution the host checks the actual `HostOp` against grants; a manifest cannot
grant itself a capability.

## Alternatives

1. One global dynamic map of strings. Rejected: no static signature, no arity
   checks, and unknown names would silently resolve after deployment.
2. Keep three lookup sites and synchronise them by convention. Rejected: the
   M0.5 audit (E02) shows they already diverge.
3. Make builtins dynamic too. Rejected: builtins are language semantics and
   their list is deliberately closed. Only the resolver is unified.
4. Let `run` probe ABI exports at execution time without a static descriptor.
   Rejected: `check` must be able to reject bad calls and must never `dlopen`.

## Compatibility

`BUILTINS`, names and arity stay unchanged. Existing Marqdo function calls keep
working through the registry. Unknown calls that M0.5 treated as plugin calls now
require a declared plugin descriptor; this is a deliberate diagnostic
improvement, not a language change. JSON diagnostics keep stable codes and add
`callable.unknown`/`callable.ambiguous` only where the old code had no specific
answer.

## Security

The registry is metadata, not authority. A descriptor only says "this callable
exists and has this signature"; the host checks `check_plugin`, the package
identity and the current `ExecutionContext` before invoking native code.
`check` must not load a dynamic library, spawn a kernel or execute imported
modules.

## Testing

* frontend tests: builtin/function/plugin descriptors, duplicate qualified
  names, aliasing, unknown member calls, arity errors;
* runtime tests: bound `CallableId` dispatch for all three origins;
* CLI contract test: a plugin export's signature is rejected statically before
  any library is loaded;
* a plugin with two prefixes exercises both descriptors.

## Migration & rollout

R1 introduces the descriptor/registry and builtin+function resolution. R2 adds
ABI export descriptors, multi-export routing and the capability broker. The
old ad-hoc plugin fallback is removed when the R2 tests prove the replacement.
