# RFC-CM P2 partial evidence: `.mq.md`-written codec, allocator and list

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0/P1/P2 changes.
Related: [ADR-0022](../adr/0022-addressable-memory-and-place-assignment.md),
[P1 evidence](2026-11-20-memory-p1.md),
[RFC-CM-0002 §7.2](../../docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md).
Scope: **partial P2 only**. The RFC-CM-0002 P2 gate is **not** passed.

> Historical slice: this report predates ADR-0023 (CM-A04 structure API),
> ADR-0024 (CM-A05 Result), the coalescing allocator (fixture 55) and the
> binary tree/CSV fixtures (56–57). Its "what remains" list is superseded by
> [`2026-11-20-structure-api-p2.md`](2026-11-20-structure-api-p2.md) and
> [`2026-11-20-result-and-free-list-p2.md`](2026-11-20-result-and-free-list-p2.md);
> the P2 gate itself is still not passed.

## What was added without Rust algorithm code

Three semantic fixtures are ordinary `.mq.md` documents. They call only
language builtins (`memory.new/size/grow`), indexing, arithmetic, functions,
branches and loops. No Rust crate was changed for the algorithms themselves.

| Fixture | `.mq.md` subject | Result |
|---|---|---|
| `tests/conformance/semantics/48-memory-bump-allocator.mq.md` | big-endian u32 encode/decode and a 4-byte-header bump allocator; allocates 8, 16, 8 bytes and writes a byte | `[4, 12, 28, 36, 255]` |
| `tests/conformance/semantics/49-memory-linked-list.mq.md` | allocates three 8-byte nodes (`value:u8`, `next:u32`), links them and traverses the list | `[4, 12, 20, 60]` |
| `tests/conformance/semantics/50-memory-allocation-failure-atomic.mq.md` | a too-large allocation returns `None` without moving the header; the next allocation still starts at 4 | `[None, 4, 4, 65536]` |

Observed CLI results (all `success: true`):

```text
48: [4, 12, 28, 36, 255]
49: [4, 12, 20, 60]
50: [None, 4, 4, 65536]
```

`cargo test -p marqdo-cli --test conformance_semantics` passes with these three
fixtures included.

## Why this is not the P2 gate

RFC-CM-0002 §9/P2 requires C01–C06 and C11 to pass in an environment with
Foreign/native plugins disabled, and requires a minimal API for dynamic
List/Map/Table construction and observation plus recoverable errors. This
slice does **not** provide:

* duplicate-free/allocator adversarial tests (C01 now has a first-fit
  free-list allocator with reuse, splitting and coalescing, fixture 55);
* graph/hash-table structures (C02 now has a singly linked list and a binary
  search tree, fixtures 49 and 56, but no graph/hash table);
* general List/Map/Table construction, mutation-free rebuilding or key
  enumeration (CM-A04 is undecided);
* a `Result`/recoverable-error contract for parser/IO/EOF/partial conditions
  (CM-A05 is undecided);
* an independent `marqdo-stdlib` repository, version/digest contract or
  frozen-CLI-binary closure lane (the RFC §10.5 experiment).

It also does not add a stdlib API to the CLI; the fixtures are conformance
documents, not a shipped standard library.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
for f in 48-memory-bump-allocator 49-memory-linked-list \
         50-memory-allocation-failure-atomic; do
  cargo run -q -p marqdo-cli -- run \
    tests/conformance/semantics/$f.mq.md --json
done
cargo test -p marqdo-cli --test conformance_semantics
```
