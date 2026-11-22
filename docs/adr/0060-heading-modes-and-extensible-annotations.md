# ADR-0060: Heading modes are core; heading annotations are declarative metadata

Status: Accepted (RFC-EXT-0003 H-05/H-07/H-13, post-`3.0.0-beta.1`).
Implementation status: **core-mode classification is implemented and tested;
Heading Annotations v1 is Proposed and not implemented in this build** (targeted
at `3.1.x`). This ADR does not itself make annotations available.
Related: RFC-LANG-0002, ADR-0052, [`spec/mapping.md`](../../spec/mapping.md),
[`spec/heading-annotations-v1.md`](../../spec/heading-annotations-v1.md).

## Context

`@exec`/`@knowledge` look like decorators, which invites the idea that a plugin
could redefine them, or add `@agent`/`@test` decorators that change whether a
section executes. That would make the same document executable in one installation
and inert in another, break `check` ↔ `run` binding equality, and let third-party
code affect capability and safety. At the same time, tools genuinely need
non-executable heading metadata (test selection, View folding, proof
obligations).

The current parser (`crates/marqdo-frontend/src/units.rs::heading_declaration`)
already treats a heading's first plain `Text` prefix as a closed mode token and
rejects any other leading `@foo` with `sem.invalid_mode_marker`
(`tests/negative/01-invalid-mode-marker-case.expected.json`). Analysis v2
`units[]` projects only `id/kind/name/level/span/function`; a `section`,
`knowledge` or `namespace` unit does not carry its effective mode or annotation
source.

## Decision

1. **Two layers, one executor.**
   - **Core Mode** is the closed set `{@exec, @knowledge}`. It alone decides
     whether a heading is callable/executable. Unmarked headings inherit the
     nearest ancestor mode (see `spec/mapping.md` §1.2, including the H-06
     bugfix). There is no third executable mode and no plugin-defined mode.
   - **Heading Annotations** are an open, declarative, metadata-only extension
     layer. They never decide execution.
2. **Annotation syntax v1** (Proposed): an ATX heading's first plain `Text` prefix
   may contain at most one core mode followed by zero or more
   `@namespace.identifier` annotations, then the semantic title. No parameters in
   v1; annotations do not inherit; `semantic_name`/`FunctionId`/anchors are derived
   only from the core mode + normalized title. A malformed/unknown annotation is a
   static error (`annotation.unknown` or `annotation.malformed`), not silently
   plain prose; ordinary body text may discuss `@foo`, and a heading-only spelling
   escape is part of the proposal.
3. **No authority.** An annotation may not add/remove callability, rewrite the AST
   or IR, reorder statements, change budgets, grant a capability, load native code
   or change an already-bound CallBinding. The runtime ignores annotations.
4. **Registration is data, not parse-time code.** Allowed annotations come from
   explicitly declared, version-pinned project/dependency resources (a static
   manifest referenced from `marqdo.yaml`), bound to project/lock identity and
   digest. Duplicate ownership, conflicting names, unknown versions and digest
   mismatches fail at `check` without executing anything.
5. **Analysis projection is versioned.** Do **not** silently add
   `effective_mode`, `mode_origin` or `annotations` to Analysis v2 `units[]`.
   Use a new Analysis v3 or a separate `heading-annotations.v1` sidecar carrying
   unit id, effective core mode, explicit/inherited origin, annotations, source
   module/span, function/unit id, and manifest owner/digest. Existing
   Analysis v2 consumers continue to validate unchanged.
6. **Step split.** Step A (accepted here) is this contract + parser test design +
   identity/schema prototype; Step B (3.1) implements descriptor loading, index,
   conflict checks, the sidecar/v3 projection and independent consumers. Until Step B
   ships, docs/STATUS must say Proposed/not implemented.

## Consequences

- Core-mode behaviour (`@exec`/`@knowledge` + ancestor inheritance) is stable and
  cannot be overridden by installed tools; the H-06 fix changes only the buggy
  sibling case.
- Tool authors get a safe metadata channel without a plugin parse hook; the runtime
  never needs to know tool names.
- Analysis v2 stays byte-compatible; new metadata needs a versioned sidecar/v3 and an
  independent black-box consumer before it can be claimed.
- `check` and `run` still agree on every call site regardless of which tools are
  installed.
