# RFC-CM P4 evidence: cross-storage table equivalence

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `e981168` plus this step's changes.
Decision record: [ADR-0032](../adr/0032-table-logical-value-and-stdlib-algorithms.md)
Related: [CM-A08 table](2026-11-20-table-cm-a08.md), [STATUS §5](../STATUS.md).
Scope: logical Table (row store T0) vs a `.mq.md` columnar store.

## Fixture

`tests/conformance/semantics/72-table-columnar.mq.md`:

* `列存 = {列: [...], 数据: [[column0...], [column1...]]}` is a columnar
  representation built from List/Map;
* `转表` converts columnar -> logical `Table`;
* `列存排序` sorts the columnar representation by index order without a Table
  value; `列存过滤` filters columns by retained indices;
* the same operations run on the logical Table via `std:data/table`;
* `表相等` compares headers + every cell (Table `==` is not structural in this
  language version, so the fixture compares cells explicitly).

Result:

```json
[true, true, true]
```

i.e. conversion is lossless, and sorted/filtered outputs are identical between
the row-store Table and the columnar `.mq.md` representation.

## What this proves / does not prove

Proves: C06 algorithms are defined by logical Table semantics, not by the
current `Vec<Vec<Value>>` layout; a columnar `.mq.md` implementation can produce
the same observable result without changing the GFM AST or Rust runtime.

Does not prove or implement: T1 controlled storage interface, T2 native column
store, T3 Memory-backed numeric columns, zero-copy views, or production
performance. Those remain P6/future work; the current Rust T0 storage is
unchanged.

## Gate

Local container: fixture passes via `conformance_semantics` and the full
workspace gate; GFM rendering (`转GFM`) is covered by
`crates/cli/tests/stdlib_table.rs`.
