# ADR-0022: Addressable `Memory` and Memory-index place assignment (RFC-CM P1)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_MACHINE_RFC_0001.md` (CM-A01/CM-A02/CM-A03),
`docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md` (P0/P1), ADR-0007,
ADR-0011, ADR-0017, ADR-0021
Supersedes: the v1 rule that assignment targets are bare identifiers only
Specs: `spec/expression.ebnf`, `spec/evaluation.md`, `spec/value-wire.md`,
`spec/analysis-v2.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-memory-p1.md`

## Context

RFC-CM-0001/0002 define three domains: managed values, addressable regions and
capability resources. The first two are a language change; the third is an OS
boundary and is deliberately **not** part of this ADR.

Before this decision the interpreter had no writable place other than a local
slot. `Value` had no byte region, `a[i]` was read-only, and assignment was
parsed only as `identifier = expression`. A `.mq.md` program therefore could
not express an allocator, an encoded record, a free list or a byte protocol
without a new Rust builtin for every algorithm. That is the closure gap
RFC-CM-0002 §2 identifies.

The decision must not:

* turn managed `List`/`Map`/`Table` values into shared mutable objects;
* expose a host pointer, `usize` address or procedural `free(region)`;
* let an active handle cross the value wire or an ABI boundary;
* add a Markdown node, keyword or `HostOp` for memory;
* claim block-level memory safety for user allocators;
* weaken `strict` or grant any external I/O.

## Decision

### 1. `Value::Memory` is an opaque, run-scoped identity

`MemoryHandle` (`crates/marqdo-value/src/value.rs`) is
`(run_id, region_id, generation)` with private fields and a runtime-only
constructor. It is not `Serialize`/`Deserialize`, is never built from user
input, and carries no host address.

Copying the value, passing it as an argument and storing it in a list or map
all copy the handle and preserve region identity. `==`/`!=` compare identity
only; `Memory` has no ordering comparison. `display()`/`repr()` render the
literal `Memory`, never the internal identity.

### 2. Regions are grow-only, bounded byte arrays

Frozen semantics (CM-A01/CM-A02):

| Property | Decision |
|---|---|
| Page size | 64 KiB logical page (`PAGE_SIZE`) |
| Initial content | Every byte is `0` |
| Growth | `memory.grow` only increases; old offsets/bytes unchanged; new bytes zero |
| Failure atomicity | A rejected create/grow allocates, commits and mutates nothing |
| Shrink/free | No shrink and no user-visible region free; regions are dropped with the run |
| Address | `(region identity, non-negative offset)`; an integer alone is never a credential |
| Read | `m[i] -> Int(0..=255)` |
| Write | `m[i] = byte`, `byte` must be `Int(0..=255)`, no truncation |
| Isolation | Different regions never alias; handle copies of one region do |
| Visibility | `memory.size` returns committed pages, not host capacity or pointers |

Limits in `MemoryLimits`:

* `DEFAULT_MAX_TOTAL_BYTES = 64 MiB` committed across the run;
* `DEFAULT_MAX_REGION_BYTES = 16 MiB` per region, which also caps declared
  `max_pages` at 256;
* budget arithmetic uses checked `u64`/`usize` operations; allocation uses
  `try_reserve` so a host OOM is a structured `resource.exhausted` instead of
  a panic.

Limits are a runtime API (`marqdo_runtime::MemoryLimits`, `run_with_limits`)
and are overridable on `run`/`call` with `--max-memory <BYTES>` (total) and
`--max-region-memory <BYTES>` (per region). The defaults are stated in
`spec/evaluation.md`. Both flags apply in `pure`, `project` and `strict`; they
grant no external capability.

### 3. The only new writable place is a `Memory` index

`AssignTarget` gains `Index { base, index, span }` in the expression AST and
`PlaceTarget` mirrors it in the IR. The surface syntax remains a `strong`
node containing `target = expression`; no Markdown role, heading or list
meaning changes.

* `x = value` still rebinds the current frame's local slot.
* `m[i] = byte` evaluates **base, then index, then RHS**, exactly once each.
  All three checks (base is `Memory`, offset in range, byte in `0..=255`)
  happen before the single byte write, so a failed statement does not write.
* `list[i] = value`, `map[key] = value` and `obj.field = value` are not
  defined. Member targets are a parse error; a non-`Memory` index base is
  `runtime.type`. No managed collection changes value semantics.
* Nested index targets are accepted syntactically; the innermost base must
  still evaluate to a `Memory` value at run time.
* Assignment remains statement-only; `x = y = 1` is still a parse error.

This is CM-A03.

### 4. Handles cannot cross the wire or the host boundary

* `to_wire(Memory)` emits only `{"type":"memory"}` so an accidental transfer
  is visible. `from_wire` rejects that tag with `WireError::Invalid`; a
  round-trip is intentionally impossible (no `wire_version` bump: the value
  shape is rejected, not newly accepted).
* `Host::invoke` rejects any argument tree containing `Memory` before dispatch
  (`runtime.type`), so plugins, foreign kernels, math, file and print adapters
  never see a handle. A future explicit byte-snapshot operation must copy bytes
  into a managed value and is a separate ADR.
* `HostOp` is unchanged. `memory.new/size/grow` and index read/write are
  interpreter operations, not capability-gated host effects.

### 5. Error categories are stable and distinct

| Situation | Code |
|---|---|
| Negative/unrepresentable offset, offset >= region length | `runtime.index` |
| Unknown, stale or cross-run handle; invalid page count (negative, > budget declaration arithmetic) | `runtime.memory` |
| Wrong operand type; byte outside `0..=255`; non-Memory index-assignment base | `runtime.type` |
| Declared max, region budget or total budget exceeded | `resource.exhausted` |

No operation panics on malformed input or forged handles. `expect` is used only
after the same handle has been validated in the same call frame.

### 6. Builtins are part of the language surface, not `HostOp`s

`memory.new(initial_pages, max_pages) -> Memory`,
`memory.size(memory) -> Int`, `memory.grow(memory, pages) -> Int` are added to
`marqdo-ir::builtins::BUILTINS` and `spec/evaluation.md`. They are pure
(`effectful: false`): `pure` mode may use them, `strict` needs no new provider,
and no capability grant is consulted.

### 7. Compatibility

* Local assignment IR keeps its `target` string in Analysis v2; an index
  target is an additive object (`{"kind":"index",...}`). Consumers that only
  handle locals continue to work.
* Expression AST/statement serde gains an enum tag; this is a pre-1.0 internal
  contract documented in `spec/expression.ebnf`.
* Existing Markdown AST goldens, value semantics, `List`/`Map`/`Table`
  behaviour and all old builtins are unchanged.
* `Memory` is additive to the value type set. Old programs never produce it.
  ABI wire consumers see a new rejected tag, not a renumbered existing tag.

## Non-goals

* No `Pointer` value, raw pointer, `free(region)`, registers or machine code.
* No index/field assignment for managed collections; no implicit mutation.
* No multi-threaded sharing, atomics or data-race guarantees.
* No serialization of live regions; no cross-process shared memory.
* No `ResourceHandle`, `Stream`, files, network or OS syscalls. Those are
  CM-A05/CM-A06/CM-A09/CM-A11 and later RFC phases.
* No block-level safety claim for a user-written allocator; the runtime only
  bounds a single access to its region.
* No allocator or stdlib API in P1; `--max-memory` only bounds the runtime.

## Consequences

* A `.mq.md` program can now build byte encoders, arenas, free lists and
  virtual CPU register files without new Rust code. P2 (the Marqdo-written
  stdlib and closure fixtures) becomes testable.
* The interpreter owns one `MemoryStore` per run; `run_with_limits` exposes
  the budget. This is runtime state, not a second `HostOp` table.
* Trace gains `memory.new`, `memory.grow` and `memory.write` semantic events
  without exposing handles or byte secrets.
* The remaining RFC decisions (CM-A04 and later) stay separate. In particular,
  generic structural observation/construction, recoverable `Result`, table
  algorithm migration and the resource/Stream ABI are **not** implemented by
  this ADR.

## Verification

`docs/evidence/2026-11-20-memory-p1.md` records:

* parser/AST tests for index and rejected member targets;
* runtime store tests for isolation, aliasing, zero-fill, atomic failed grow,
  negative/huge offsets, forged/stale handles, byte range and budgets;
* semantic goldens `45`–`47` and runtime negatives `13`–`18`;
* wire rejection test and host-boundary rejection test;
* CLI example `examples/memory-buffer.mq.md` returning `141`;
* Analysis v2 index-target projection;
* `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`.
