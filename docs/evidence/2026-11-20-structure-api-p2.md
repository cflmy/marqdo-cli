# RFC-CM CM-A04 evidence: minimal structure observation and construction

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0/P1/P2 changes.
Decision record: [ADR-0023](../adr/0023-minimal-structure-observation-and-construction.md)
Related: [P1 memory evidence](2026-11-20-memory-p1.md),
[P2 closure evidence](2026-11-20-memory-p2-closure.md),
[STATUS §5](../STATUS.md).
Scope: CM-A04 only. The full RFC-CM-0002 P2 gate is **not** passed.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                                        PASS
cargo clippy --workspace --all-targets -- -D warnings             PASS
cargo test --workspace --locked                                   PASS
  47 test binaries, 389 passed, 0 failed, 2 ignored (doc-tests)
```

The 2 ignored tests are the pre-existing `marqdo-plugin-sdk` doc-tests. No
GitHub CI was available from this checkout; this is local evidence.

## Builtins added (pure, no `HostOp`, no capability)

| Name | Semantics |
|---|---|
| `list.append` | new list with one appended value; input unchanged |
| `list.concat` | new concatenated list; inputs unchanged |
| `map.keys` | Text keys in canonical sorted order |
| `map.has` | key membership |
| `map.get` | stored value or `None`; `map.has` disambiguates a stored `None` |
| `map.set` | new map with the key; input unchanged |
| `map.remove` | new map without the key; missing key is a no-op |
| `text.codepoints` | Unicode scalar values as `Int`, in order |
| `text.from_codepoint` | one-scalar Text, or `None` for surrogate/out-of-range |
| `table.columns` | header names in column order |
| `table.row` | new list of row cells; non-negative coordinate |
| `table.cell` | cell value; non-negative row/column |

No `sort`, `filter`, `join`, `group`, JSON/CSV, `table.set_cell`, mutable
List/Map or new Markdown syntax was added. Constructors never mutate their
arguments; this preserves INV-02.

## Semantic fixtures

```text
51-list-map-value-semantics:
  original list [1,2] unchanged; appended [1,2,3]
  original map {a:1,b:2} unchanged; map.set c=3 then remove a
  keys after remove => ["b","c"]
  map.has(original, "c") => false
  map.get(original, "z") => None
  list.concat([1],[2,3]) => [1,2,3]

52-text-codepoints:
  text.codepoints("aé中") => [97, 233, 20013]
  text.from_codepoint(20013) => "中"
  text.from_codepoint(0xD800) => None
  text.from_codepoint(0x110000) => None

53-table-projection:
  columns => ["名称","分数"]
  row 1 => ["乙",92]
  cell(0,1) => 87
  长度(table) => 2

54-table-filter-stdlib:
  原表 3 rows, threshold 80
  结果 table has 2 rows (甲 87, 乙 92)
  原表 is still 3 rows and byte-for-byte the same logical table
```

Fixture 54 is the first `.mq.md` algorithm over the new primitives: it uses
`table.columns`, `table.cell`, `table.row`, `list.append` and `表` to rebuild a
filtered table. It is a conformance document, not a shipped `table.filter`
builtin; no Rust table algorithm was added.

CLI confirms all three return `success: true` with the checked-in wire values.

## Runtime negatives

| Fixture | Expected code | Observed |
|---|---|---|
| `19-list-append-wrong-type` | `runtime.type` | `runtime.type` |
| `20-table-cell-out-of-bounds` | `runtime.index` | `runtime.index` |
| `21-map-key-not-text` | `runtime.type` | `runtime.type` |

## Focused command

```text
cargo test -p marqdo-cli --test conformance_semantics --test negative
  semantic_goldens_match_end_to_end_execution ... ok
  runtime_negative_goldens ... ok
```

## What remains open

* CM-A05 recoverable `Result`/EOF/partial model. This report predates ADR-0024,
  which now freezes the `Result` map convention; the Stream EOF/partial part
  remains CM-A06. `map.get` returning `None` is a lookup convention, **not**
  that contract.
* CM-A06 `Resource`/`Stream` ABI, CM-A07 stdlib naming/alias, CM-A08 Table
  storage/physical migration, CM-A09 provider ABI, CM-A10 stdlib packaging,
  CM-A11 strict resource policy.
* A separate `marqdo-stdlib` repository, graph containers, a full JSON parser,
  quoted/escaped CSV and the RFC §10.5 frozen-CLI closure lane.
* Windows/macOS native lanes. No remote CI URL exists for this working tree.
