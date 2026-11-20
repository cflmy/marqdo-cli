# ADR-0010: Register every plugin export by its complete name

Status: **Accepted**
Date: 2026-11-20

## Context

`crates/marqdo-host/src/host.rs::load_plugin` binds one `PluginSlot` using
`manifest.plugin.exports.first()`. The namespace is the text before the first
`.` and `op_plugin_invoke` looks it up with `find(namespace)`. Consequences:

* a plugin exporting `solver.add` and `graph.search` can only be reached through
  one of the two prefixes;
* two plugins declaring the same prefix race, and whichever slot was pushed
  first wins;
* the frontend has no per-export identity to validate, and a call is effectively
  "first namespace match".

The M0.6 audit E03 and plan §10 require complete-name registration for every
export.

## Decision

### 1. Export registration

`StdHost` keeps a registry keyed by the complete export name:

```rust
struct PluginExport {
    export: String,             // `solver.add`
    package: String,            // `qdqc.solver`
    manifest_digest: String,    // sha256 of the canonical manifest/artifact metadata
    signature: CallableSignature,
    effects: Vec<String>,
    plugin: Rc<LoadedPlugin>,   // shared by all exports of one loaded library
}
```

Loading a plugin validates the manifest, loads the library **once** and inserts
one entry per export. Insertion is atomic for the whole manifest: if any export
is already registered with a different package/manifest, loading fails and the
previous registry state is unchanged. A re-load of the identical
package+manifest may reuse the existing library.

There is no `exports.first()` and no namespace-only lookup. A call uses the
complete dotted name from `CallableId` / the IR call site.

### 2. Aliases

If two packages intentionally provide the same export name, the user must
declare an explicit alias in the importing manifest/front matter (for example
`solver.add as fast.add`) or the call is ambiguous. M0.6 may implement only the
hard-conflict diagnostic; the alias syntax is additive and must not silently
choose a winner.

### 3. Runtime binding

The frontend resolves `solver.add` to a `CallableDescriptor` (ADR-0009) and the
runtime passes the `CallableId` to the host. The host looks up exactly that id;
it never re-resolves a bare namespace.

The static descriptor must include the manifest/artifact digest so a call
checked against one plugin version is rejected if the installed library changed
between `check` and `run`.

### 4. Worker parity

The isolated worker transport (R2) registers the same complete export names and
the same descriptor table. A plugin's routing must not depend on whether it is
loaded natively or in a worker.

## Alternatives

1. Keep one namespace per plugin and require authors to use one prefix.
   Rejected: the manifest already permits multiple namespaced exports and the
   audit found silently unreachable functions.
2. Let the host pick the first matching namespace. Rejected: nondeterministic
   and spoofable.
3. Reject duplicate export prefixes at manifest validation. Rejected: this
   forbids legitimate multi-package ecosystems and does not solve identical
   names across different packages.

## Compatibility

Existing single-namespace plugins continue to work; their export is registered
by complete name. Calls that previously relied on "first prefix" behavior now
require the exact export name. No C ABI layout change is needed for native
loading. The host's public `load_plugin` return value becomes a summary of all
registered exports rather than one namespace.

## Security

Registration is not authorization. The manifest digest is recorded so callers
can reject a swapped binary. Capability grants remain per plugin package and are
checked on each invocation (ADR-0011). A conflicting export never replaces an
existing registration.

## Testing

* one plugin with `solver.add` + `graph.search` can call both;
* two plugins exporting `solver.add` produce a deterministic conflict and do not
  overwrite;
* a plugin manifest with duplicate export names is rejected during validation;
* a manifest artifact digest mismatch is rejected at load and at call binding;
* worker and native transports expose the same complete-name table.

## Migration & rollout

R2 implements complete-name registration together with the worker transport.
`spec/plugin-abi.md` is updated; the old namespace helper is removed after the
new tests pass.
