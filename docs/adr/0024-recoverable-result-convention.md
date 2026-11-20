# ADR-0024: Recoverable result convention (RFC-CM CM-A05)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_MACHINE_RFC_0001.md`,
`docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md` (CM-A05, P2/P3),
ADR-0023
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-20-result-and-free-list-p2.md`

## Context

A standard library cannot be written in `.mq.md` if every expected condition
(no free block, invalid input, missing field, later end-of-file or a short
read) must abort the whole run. The core has structured `RuntimeError`s with
stable codes, but no way for an ordinary function to return a failure that the
caller can inspect and handle in a normal branch. RFC-CM-0002 §3.6 recommends a
`Result` data contract using existing `Map` values and control flow, without
`try`/`catch` or exception syntax.

This ADR freezes the **data convention**, not a new runtime type. A separate
core `Result` value, algebraic types or automatic propagation would be a
larger change and is deliberately not introduced.

## Decision

### 1. Shape

A recoverable result is an ordinary `Map`:

```text
success: {ok: true,  value: <any Value>}
failure: {ok: false, code: <Text>, message: <Text> [, details: <Map>]}
```

* `ok` is always present and always `Bool`.
* On success, `value` is present. It may itself be `None`, a `List`, a `Map`,
  a `Table`, a `Memory` handle or any other value.
* On failure, `code` and `message` are present. `code` is a lowercase
  dot-separated stable identifier; `message` is human-readable text.
* `details` is optional and, when present, is a `Map` of additional structured
  facts. It must not contain host pointers, resource handles or secret bytes.
* No other fields are required. Consumers must tolerate additive fields.

The constructors are ordinary `.mq.md` functions, for example:

```text
## 成功
`(值)`
*{ok: true, value: 值}*

## 失败
`(代码, 消息)`
*{ok: false, code: 代码, message: 消息}*
```

They are **not** builtins. A library may choose different local function names;
the map shape is the contract.

### 2. What is recoverable, what stays fatal

Recoverable (`Result`):

* a user allocator has no fitting block (`alloc.no_space`);
* a parser rejects malformed input or hits an unexpected end
  (`parse.invalid_syntax`, `parse.unexpected_eof`);
* a codec rejects an invalid sequence (`codec.invalid_sequence`);
* a lookup that uses an explicit result because `None` is a legal value;
* later, a resource reports EOF/partial/pending as a **success payload** with
  its own byte counts, not as a fatal error (P3 contract).

Fatal `RuntimeError` (unchanged):

* violating a language invariant: wrong operand type, out-of-bounds managed
  collection access, unknown/stale `Memory` handle, memory budget exhaustion,
  division by zero, call arity, unknown symbol;
* a capability denial (authorization is not a recoverable business result);
* an internal invariant failure.

A library must not convert a fatal invariant into an `Err` by catching it;
there is no catch mechanism and none is proposed here. It must validate inputs
before calling an operation whose failure is fatal, or rely on its own
pre-checks.

### 3. Codes and namespaces

`code` is stable within the library that defines it. Recommended prefixes:

| Prefix | Owner |
|---|---|
| `alloc.*` | allocator libraries |
| `parse.*` | parsers |
| `codec.*` | byte/text codecs |
| `table.*` | table algorithms |
| `io.*` | stream wrappers (P3, when the resource contract exists) |

Codes are compared as `Text`; a caller must not depend on the `message` text.
Libraries may add codes; removing or renaming one is a breaking change for that
library.

### 4. EOF and partial I/O (forward contract)

When the Stream/Resource API arrives (CM-A06), a successful read must
distinguish:

```text
{ok: true, value: {count: Int, eof: Bool, pending: Bool}}
```

EOF is not an `Err`; a short read is not an `Err`; a connection reset or a
permission failure is. The exact stream payload is frozen by CM-A06, not by
this ADR. This ADR only reserves the principle.

### 5. Why not a new type or syntax

* A dedicated `Value::Result` would change the value model, wire format,
  display, equality and every exhaustive `Value` match for a convention that
  existing `Map` plus `ok` already expresses.
* `try`/`catch` would add a control-flow construct that bypasses ordinary
  branches, contradicts the RFC's preference for `Result` values and needs new
  grammar, scope and error rules.
* Ordinary `Map` results are visible to `type`, `length`, `map.get` and wire
  encoding today, and can be built by `.mq.md` libraries without a new ABI.

## Compatibility

* Additive: existing builtins and programs are unchanged.
* `map.get` returning `None` for a missing key remains a lookup convention,
  not a `Result`; callers that need to distinguish a stored `None` use
  `map.has` (ADR-0023).
* Runtime error categories and codes are unchanged.
* No wire version bump: a `Result` is an ordinary `Map` already covered by the
  wire format.

## Non-goals

* No automatic `?`/propagation, exceptions, panic recovery or stack unwinding.
* No `Result` type, generics, pattern matching or exhaustive `Ok`/`Err`
  typing.
* No change to `memory.grow` or other core operations from fatal errors to
  `Result`; a library may pre-check and return `Err` instead.
* No resource/stream implementation; CM-A06 remains open.
* No promise that every existing library will adopt the convention.

## Consequences

* `.mq.md` algorithms can return and branch on failure without aborting the
  run. This is a prerequisite for a Marqdo-written allocator, parser and
  eventually `io.read_line`.
* Because the convention is untyped, a caller can inspect the wrong shape;
  `类型(result)`, `map.has(result, "ok")` and the frozen field names are the
  checks. A future typed-value ADR may improve this without breaking the data
  shape.
* The allocator fixture in `54`/`55` demonstrates success and failure maps and
  ordinary branching, with no Rust algorithm.

## Verification

`docs/evidence/2026-11-20-result-and-free-list-p2.md` records a `.mq.md`
first-fit/free-list allocator that returns `Result` maps, reuses freed blocks,
splits free blocks, coalesces address-adjacent free blocks and returns
`alloc.no_space` without aborting the run.
