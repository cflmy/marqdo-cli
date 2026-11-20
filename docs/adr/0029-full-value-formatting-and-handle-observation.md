# ADR-0029: Full-value stdlib formatting and handle observation (CM-A04/A07)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A04, CM-A07, P2/P3), ADR-0023, ADR-0027, ADR-0028
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-20-full-value-formatting.md`

## Context

Compat-A (ADR-0027) formats Unit/None/Bool/Int/Float/Text/List/Map/Table, but
builtin `打印` also formats Formula, Code, FunctionRef and PluginHandle. A
Compat-B name route cannot be honest while the stdlib silently differs on
those types. The missing pieces are the **identity/observation** fields needed
to reproduce `Value::repr()` for function references and plugin handles;
Formula and Code already expose `latex/display/unit` and
`language/source/content_hash/unit`.

## Decision

### 1. Read-only identity members

Add member reads (no new value types, no mutation, no handles):

| Value | Members |
|---|---|
| `FunctionRef` | `module: Text`, `name: Text` |
| `PluginHandle` | `plugin: Text`, `export: Text` |

The host-assigned numeric `PluginHandle.id` remains private; only the manifest
plugin name and export name are exposed, which are already visible in
`Value::repr()` and are not a capability token. `Memory` and `Resource` remain
opaque and expose no fields. Unknown members are `runtime.unknown_field`.

### 2. Stdlib formatting covers all 15 value types

`stdlib/format.mq.md` gains branches for:

* `Formula`: `Formula(<debug-quoted latex>)`;
* `Code`: `Code(<debug-quoted language>, <utf8-byte-length> bytes)`
  (the byte length is computed with `stdlib/text.mq.md`, not `长度`, which is
  Unicode scalar count);
* `FunctionRef`: `FunctionRef(module::name)`;
* `PluginHandle`: `PluginHandle(plugin::export)`;
* `Memory`/`Resource`: the literal `Memory`/`Resource`;
* `Unit`: `Unit`.

The existing `Text`/collection escaping (quotes, backslash, newline, tab, CR)
is reused. Control characters outside those escapes are not emitted as
`\u00XX` in this slice; that is a formatting limitation, not a security
boundary, and does not affect the current corpus.

### 3. Compatibility

* The new members are additive observations; no existing expression changes.
* `PluginHandle.id` is not observable and cannot be used to forge a handle.
* `打印` remains the builtin default; this ADR only makes the stdlib formatter
  capable of matching it for all value types. Compat-B routing is a separate
  decision (ADR-0030).
* Formula/Code provenance spans are not exposed as members; only the fields
  already present in the value wire/repr are.

## Non-goals

* No mutable handle inspection, no resource/plugin capability escalation.
* No `\u00XX` escaping of arbitrary control characters.
* No change to `Value::display()`/`repr()` themselves; the stdlib must match
  the existing behavior, not redefine it.
* No print name routing in this ADR.

## Consequences

* The stdlib formatter can reproduce `打印` output for all 15 current value
  types, removing the Compat-B blocker documented in ADR-0027.
* Function/plugin identity becomes readable in `.mq.md`; this is already public
  in display/repr and does not reveal the host-only numeric handle id.
* Code byte length performs one UTF-8 encode into a temporary `Memory`; large
  code values cost an extra allocation during formatting. A future core
  observation could avoid it without changing output.

## Verification

`docs/evidence/2026-11-20-full-value-formatting.md` records the member-read
tests, a Formula/Code/FunctionRef formatting comparison against builtin
`打印`, and the full workspace gate.
