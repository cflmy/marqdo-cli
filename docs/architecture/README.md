# Architecture RFCs: compute machine and stdlib migration

Status: **Draft RFCs, partially implemented**. This page is an index; the
normative RFC text stays at its reviewable path under `docs/`.

| Document | Status | Implemented part |
|---|---|---|
| [RFC-CM-0001: compute machine, addressable memory and system boundary](../MARQDO_COMPUTE_MACHINE_RFC_0001.md) | Draft / design input | R1 (P1): `Memory` value, `memory.new/size/grow`, byte read/write, Memory-index assignment, budgets, wire/host rejection. Implemented per [ADR-0022](../adr/0022-addressable-memory-and-place-assignment.md); see [STATUS §5](../STATUS.md). |
| [RFC-CM-0002: minimal core, stdlib self-hosting and system capability migration](../MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md) | Draft / design input | P0/P1, a **partial P2**, and a **partial P3** first slice (ADR-0025/0026: stdout write, granted project-file read and buffered project-file write via `resource.*`; strict refuses all opens; ADR-0027 Compat-A `.mq.md` print prototype, builtin still default). P2: fixtures 48–50 (`.mq.md` bump allocator, u32 codec, linked list/traversal, failed-allocation atomicity), CM-A04 structure primitives for List/Map/Text/Table (ADR-0023, fixtures 51–54), and a `.mq.md` first-fit free-list allocator with block splitting, address-adjacent coalescing and `Result` maps (ADR-0024, fixture 55), plus a `Memory`-backed binary tree with recursive insert/in-order traversal (fixture 56), a `.mq.md` CSV row splitter (fixture 57), an open-addressing hash table with linear probing (fixture 58), a `.mq.md` UTF-8 encoder and validating decoder (`utf8.truncated`/`invalid_continuation`/`invalid_scalar`) (fixtures 59–60), a `Memory`-backed graph adjacency list (fixture 61), a recursive JSON parser for arrays, objects, strings with escapes, integers/booleans/null and error Results (fixture 62), a `Memory`-backed stack bytecode VM with PUSH/ADD/MUL/JMP/JZ/HALT (fixture 63), big-endian 16/32/64-bit plus LEB128 integer codecs (fixture 64), a `.mq.md` table sort rebuilding the table without mutating the input (fixture 65), a table join built from `table.row/columns/cell` (fixture 67), and a quoted/escaped CSV row parser (fixture 68), table group (fixture 69) and an Int matrix multiply (fixture 70). No separate stdlib repo, JSON floats, network/generic providers, P4 Table migration, P5 providers or P6 performance work. |

The RFCs are architecture constraints, not evidence that a feature exists. The
implementation status lives in `docs/STATUS.md`; frozen behavior is in
`spec/`; decisions are in `docs/adr/`.

## Decisions already frozen

| RFC item | ADR | State |
|---|---|---|
| CM-A01 Memory identity/lifetime | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A02 page size, growth, budgets | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A03 place assignment and evaluation order | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A04 minimal structure observation/construction | [0023](../adr/0023-minimal-structure-observation-and-construction.md) | Accepted |
| CM-A05 recoverable `Result` convention | [0024](../adr/0024-recoverable-result-convention.md) | Accepted |
| CM-A06 minimal Resource/Stream ABI | [0025](../adr/0025-minimal-resource-stream-abi.md) | Accepted (first slice) |
| CM-A06 buffered project-file write | [0026](../adr/0026-buffered-project-file-write-streams.md) | Accepted |
| CM-A07 print migration staging / Compat-A | [0027](../adr/0027-print-migration-compat-a.md) | Accepted (Compat-A only) |
| CM-A07 Compat-B/C, stdlib packaging, Table storage, generic providers, strict resources | — | Not decided; do not implement by implication |

## What a future phase must not do

* add a top-level CLI command or Markdown keyword for memory, allocation or I/O;
* add a `HostOp` for every new stdlib function;
* turn managed `List`/`Map`/`Table` into aliased mutable storage;
* expose a host pointer, let a handle cross the wire, or claim block-level
  safety for user allocators;
* make `strict` accept an unverified provider by default;
* claim an RFC phase is complete without the closure and negative evidence its
  phase gate requires.
