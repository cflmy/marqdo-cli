# ADR-0032: GFM Table logical value, stdlib algorithms and T0 storage (CM-A08)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A08, P2/P4), ADR-0023
Specs: `spec/evaluation.md`, `spec/mapping.md`
Evidence: `docs/evidence/2026-11-20-table-cm-a08.md`

## Context

RFC-CM §6 separates three layers: the GFM source document (canonical Markdown
AST), the logical `Table` value, and the physical storage. The current runtime
uses `Value::Table { header: Vec<String>, rows: Vec<Vec<Value>> }`, which is
correct but concrete. Table algorithms already exist as `.mq.md` conformance
fixtures (filter/sort/join/group), but they are not shipped as a reusable
stdlib module. CM-A08 requires that high-level table algorithms live in the
standard library and that physical storage can change without changing
observable semantics.

## Decision

### 1. Three layers stay separate

* **GFM source**: unchanged. The Markdown parser owns `table/row/cell`,
  spans, laziness and rendering. This ADR adds no table execution to Markdown.
* **Logical value**: `Value::Table` keeps ordered headers, ordered rows, cell
  values, width validation, equality/display/wire semantics and value-copy
  behavior (`b = a` does not alias).
* **Physical storage**: T0 remains `Vec<Vec<Value>>`. This is an implementation
  detail; a future T1 controlled interface or T2 column store must preserve the
  logical contract. No column store, view or Memory-backed table is introduced
  here.

### 2. Standard-library table module

Add `stdlib/data/table.mq.md`, importable as `std:data/table`:

| Function | Semantics |
|---|---|
| `选择列(数据, 列)` | new table with one column, original unchanged |
| `过滤数值大于(数据, 列, 阈值)` | rows whose numeric cell is `>` threshold |
| `排序(数据, 列)` | insertion sort by a numeric column; new table |
| `插入有序(已排序行, 行, 列)` | helper; value-returning |
| `追加行(数据, 行)` | new table with one appended row (width validated by `表`) |
| `设置单元格(数据, 行, 列, 值)` | rebuilds rows; original unchanged |
| `连接(左, 右, 左列, 右列)` | equality join; headers concatenated |
| `分组(数据, 键列, 值列)` | Text key -> List of values |

All functions use only `table.columns/row/cell`, `list.*`, `map.*` and `表`;
none is a new builtin or `HostOp`. The module is ordinary `.mq.md`, loaded
through the verified `std:` bundle and its digest.

### 3. Semantics fixed by this ADR

* Every operation returns a **new** table; inputs are never mutated.
* Row/column coordinates are non-negative; out-of-range is `runtime.index`.
* Numeric algorithms use `Int`/`Float` comparison of cell values; passing a
  Text cell where a number is required is a runtime type error, not a silent
  coercion.
* GFM cell content such as `**不执行**` or backticked code remains inert data;
  no table function parses a cell as executable source.
* Empty tables, zero rows/columns, duplicate headers and Unicode headers keep
  their existing behavior; `表` still validates row width.
* Table source spans and document addresses are unchanged; a rebuilt Table is
  a logical value, not a rewrite of the source document.

### 4. What is deliberately not done

* No physical storage migration (T1/T2/T3), column store, shared view or
  Memory-backed table.
* No `DataFrame`/`TableView` parallel types; `Value::Table` remains the only
  table value.
* No per-operation Rust builtin (`table.filter`, `table.join`, ...).
* No mutable table update; `设置单元格` rebuilds.
* No sorting/grouping of heterogeneous mixed-type columns beyond the explicit
  numeric/Text contracts.
* No change to GFM parsing or rendering.

## Consequences

* A project can `import: {table: "std:data/table"}` and filter/sort/join/group
  tables without new Rust code; this is the P4 migration's first concrete
  stdlib artifact.
* The physical representation remains replaceable because algorithms depend
  only on the logical observation/construction API.
* Large-table performance is O(n²) for insertion sort/join and copies rows;
  P6 performance work (column store, persistent vectors) can change storage
  without changing these outputs.
* The bundle digest must be regenerated when this module changes.

## Verification

`docs/evidence/2026-11-20-table-cm-a08.md` records the module self-test and a
CLI project importing `std:data/table` that checks filter/select/append/set/
join/group results and input immutability, plus the full workspace gate.
