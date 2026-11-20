# ADR-0023: Minimal structural observation and construction (RFC-CM CM-A04)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_MACHINE_RFC_0001.md`,
`docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md` (CM-A04, P2),
ADR-0007, ADR-0022
Specs: `spec/evaluation.md`, `spec/expression.ebnf`, `spec/analysis-v2.md`
Evidence: `docs/evidence/2026-11-20-structure-api-p2.md`

## Context

RFC-CM-0002 §3.5 identifies a closure gap: a `.mq.md` program can read a
`List`/`Map`/`Table` and print it through the host, but it cannot generally
**construct** the same structures dynamically or **observe** all of their
logical parts. That makes `format`, JSON/CSV parsers and table algorithms
impossible to write in `.mq.md` without adding a task-specific Rust function
for each algorithm.

The current core already has:

* `List` literals, index reads (including negative offsets), `长度`;
* `Map` record literals, `.field` reads, `长度`;
* `Text` concatenation, character indexing and `长度`;
* `Table` construction through `表(header, rows)`, `长度` (rows), display and
  wire round-trip.

It lacks: append/concat, dynamic map update/enumeration, Unicode scalar
conversion, and table column/row/cell projection. These are observation and
construction primitives, not algorithms. Sorting, filtering, joining,
serialising and parsing remain standard-library work.

## Decision

Add a small, namespaced, **pure** builtin set. No new Markdown node, keyword,
`HostOp`, capability or mutation semantics is introduced. All constructors
return **new** values; an input collection is never modified in place. This
keeps INV-02 and preserves existing `List`/`Map`/`Table` copy semantics.

### 1. `List`

| Signature | Semantics |
|---|---|
| `list.append(list: List, value) -> List` | New list with `value` at the end; the input list is unchanged. |
| `list.concat(left: List, right: List) -> List` | New list containing `left` then `right`; both inputs unchanged. |

Neither aliases the other list's element storage: values are copied according
to their normal value semantics (`Memory` handles keep region identity).

### 2. `Map`

| Signature | Semantics |
|---|---|
| `map.keys(map: Map) -> List` | `Text` keys in canonical sorted order (`BTreeMap` order, the order used by `display`). |
| `map.has(map: Map, key: Text) -> Bool` | Whether the key exists. |
| `map.get(map: Map, key: Text) -> Value` | Stored value, or `None` when absent. Use `map.has` when `None` is a legal stored value. |
| `map.set(map: Map, key: Text, value) -> Map` | New map with the key set; the input map is unchanged. |
| `map.remove(map: Map, key: Text) -> Map` | New map without the key (missing key is a no-op); the input map is unchanged. |

Keys are always `Text`; arbitrary-value keys are not introduced. `map.get`
returning `None` is a *lookup result*, not the language's recoverable-error
contract; a full `Result` convention remains CM-A05 and is not frozen here.

### 3. `Text`

`Text` remains a Unicode scalar-value sequence, not a byte array. These
primitives make the existing representation observable without introducing a
second text type or implicit UTF-8 conversion.

| Signature | Semantics |
|---|---|
| `text.codepoints(text: Text) -> List` | Unicode scalar values (`Int`) in code-point order. |
| `text.from_codepoint(codepoint: Int) -> Text or None` | One-scalar `Text`; `None` for a surrogate or a value outside `0..=0x10FFFF`. |

UTF-8 encoding/decoding is **not** a core primitive. It can be written in
`.mq.md` using bytes, `text.codepoints` and `text.from_codepoint`; this keeps
the byte/code-point distinction explicit.

### 4. `Table`

`Value::Table` stays the logical value; its physical `Vec<Vec<Value>>` layout
is not part of this contract. Indices are non-negative row/column coordinates
(unlike `List` negative indexing) because a table cell is a document
coordinate. Bounds failures are `runtime.index`.

| Signature | Semantics |
|---|---|
| `table.columns(table: Table) -> List` | Header names (`Text`) in column order. |
| `table.row(table: Table, row: Int) -> List` | A new list of the row's cells in column order. |
| `table.cell(table: Table, row: Int, column: Int) -> Value` | The cell value. |

No `table.set_cell`/`append_row` builtin is added. Rebuilding a table is a
value-level operation that can be written with `表(header, rows)` once list
construction exists; adding a Rust table algorithm would violate the closure
goal.

### 5. Error and compatibility rules

* Wrong collection/text/table type or key type → `runtime.type`.
* Table row/column outside the logical shape → `runtime.index`.
* `text.from_codepoint` never throws for an invalid scalar; it returns `None`.
* `map.get` never throws for a missing key; it returns `None`.
* Existing `List`/`Map`/`Table` behavior, display, wire encoding and Markdown
  laziness are unchanged.
* The builtins are `effectful: false`, available in `pure`, `project` and
  `strict`, and add no capability. They do not consume output budget.
* Names are frozen as written; aliases are not added. Namespaced calls use the
  existing `Expr::Member` resolution path.

## Non-goals

* No `set_cell`, `append_row`, `sort`, `filter`, `join`, `group`, JSON/CSV or
  other algorithm builtins; those belong in `.mq.md`.
* No mutable/aliased List or Map, no in-place mutation, no shared structural
  sharing protocol.
* No arbitrary-value map keys, no ordered-map insertion semantics, no
  duplicate-key policy change.
* No UTF-8/UTF-16 conversion primitive, no byte-array text type.
* No `Result`/`try`/exception syntax; CM-A05 stays open.
* No change to Table storage, GFM parsing, source spans or `ref` semantics.

## Consequences

* A `.mq.md` program can build maps and lists dynamically, enumerate keys,
  project table cells and encode Unicode scalars. This unblocks a Marqdo-
  written formatter, table filter/projection and a JSON/CSV parser without
  per-algorithm Rust.
* The builtin table grows by 12 pure entries. The migration rule remains:
  only observation/construction primitives may enter core; algorithms do not.
* Because constructors are value-returning, repeated `map.set` in a loop is
  O(n²) for a BTreeMap clone. That is acceptable for P2 semantics; a
  persistent/structural-sharing representation is a P6 performance decision,
  not a semantic one.
* `map.get` returning `None` for absence is deliberate and documented; a
  future CM-A05 `Result` convention must not silently reinterpret existing
  programs.

## Verification

`docs/evidence/2026-11-20-structure-api-p2.md` records:

* ADR/spec deltas and builtin metadata;
* semantic fixtures for value-semantics list/map operations, code-point
  round-trips, table projection and invalid code points;
* negative fixtures for wrong types and table bounds;
* unit tests, `cargo fmt --check`, `cargo clippy -- -D warnings` and
  `cargo test --workspace`.
