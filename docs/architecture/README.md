# Architecture RFCs: compute machine and stdlib migration

Status: **Draft RFCs, partially implemented**. This page is an index; the
normative RFC text stays at its reviewable path under `docs/`.

| Document | Status | Implemented part |
|---|---|---|
| [RFC-CM-0001: compute machine, addressable memory and system boundary](../MARQDO_COMPUTE_MACHINE_RFC_0001.md) | Draft / design input | R1 (P1): `Memory` value, `memory.new/size/grow`, byte read/write, Memory-index assignment, budgets, wire/host rejection. Implemented per [ADR-0022](../adr/0022-addressable-memory-and-place-assignment.md); see [STATUS §5](../STATUS.md). |
| [RFC-CM-0002: minimal core, stdlib self-hosting and system capability migration](../MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md) | Draft / design input | P0/P1, a **partial P2**, and a **partial P3** first slice (ADR-0025/0026/0040: stdin read, separate stdout/stderr write, granted project-file read and buffered project-file write via `resource.*`; strict permits only the stdio stdout console channel; ADR-0027/0030/0031 Compat-C print via bundled stdlib, no legacy host formatter). P2: fixtures 48–50 (`.mq.md` bump allocator, u32 codec, linked list/traversal, failed-allocation atomicity), CM-A04 structure primitives for List/Map/Text/Table (ADR-0023, fixtures 51–54), and a `.mq.md` first-fit free-list allocator with block splitting, address-adjacent coalescing and `Result` maps (ADR-0024, fixture 55), plus a `Memory`-backed binary tree with recursive insert/in-order traversal (fixture 56), a `.mq.md` CSV row splitter (fixture 57), an open-addressing hash table with linear probing (fixture 58), a `.mq.md` UTF-8 encoder and validating decoder (`utf8.truncated`/`invalid_continuation`/`invalid_scalar`) (fixtures 59–60), a `Memory`-backed graph adjacency list (fixture 61), a recursive JSON parser/formatter for arrays, objects, strings with escapes, ints/floats/booleans/null and error Results (fixture 62), a `Memory`-backed stack bytecode VM with PUSH/ADD/MUL/JMP/JZ/HALT (fixture 63), big-endian 16/32/64-bit plus LEB128 integer codecs (fixture 64), a `.mq.md` table sort rebuilding the table without mutating the input (fixture 65), a table join built from `table.row/columns/cell` (fixture 67), and a quoted/escaped CSV row parser (fixture 68), table group (fixture 69) and an Int matrix multiply (fixture 70). The stdlib intentionally remains in this repository for now (ADR-0039); no separate external repo, network/generic providers, P4 physical storage migration, P5 providers or P6 performance work. |

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
| CM-A10 bundled stdlib / `std:` imports / digest | [0028](../adr/0028-bundled-stdlib-and-std-imports.md) | Accepted (first slice) |
| Full-value stdlib formatting / FunctionRef+PluginHandle observation | [0029](../adr/0029-full-value-formatting-and-handle-observation.md) | Accepted |
| CM-A07 Compat-B print routing | [0030](../adr/0030-print-compat-b-routing.md) | Accepted |
| CM-A07 Compat-C remove legacy print formatting | [0031](../adr/0031-print-compat-c-remove-host-formatting.md) | Accepted |
| CM-A08 table logical value / stdlib algorithms / T0 storage | [0032](../adr/0032-table-logical-value-and-stdlib-algorithms.md) | Accepted |
| CM-A09 in-process ResourceProvider v1 | [0033](../adr/0033-in-process-resource-provider-v1.md) | Accepted (trusted in-process slice) |
| P5 blocking TCP resource endpoint v1 | [0034](../adr/0034-tcp-resource-endpoint.md) | Accepted (IP-literal, no DNS/TLS) |
| CM-A09 worker resource-provider transport v1 | [0035](../adr/0035-worker-resource-provider-transport.md) | Accepted (trusted helper) |
| CM-A09 C ABI ResourceProvider v1 | [0036](../adr/0036-c-abi-resource-provider.md) | Accepted (trusted native adapter) |
| CM-A09 provider package manifest v1 | [0037](../adr/0037-provider-package-manifest.md) | Accepted (host-side install) |
| CM-A09 C ABI provider worker binding v1 | [0038](../adr/0038-c-abi-provider-worker-binding.md) | Accepted (trusted helper) |
| Stdlib extraction deferral | [0039](../adr/0039-stdlib-extraction-intent.md) | Accepted (in-repo for now) |
| Stdio stdin/stderr plus stdlib stream/console APIs | [0040](../adr/0040-stdio-stdin-stderr-and-stdlib-streams.md) | Accepted (stdin read, separate stdout/stderr, product stdlib) |
| Table physical storage (T1/T2/T3), strict stdin/stderr/file/network resources | — | Not decided; do not implement by implication |

## What a future phase must not do

* add a top-level CLI command or Markdown keyword for memory, allocation or I/O;
* add a `HostOp` for every new stdlib function;
* turn managed `List`/`Map`/`Table` into aliased mutable storage;
* expose a host pointer, let a handle cross the wire, or claim block-level
  safety for user allocators;
* make `strict` accept an unverified provider by default;
* claim an RFC phase is complete without the closure and negative evidence its
  phase gate requires.
