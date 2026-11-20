# RFC-CM CM-A07 Compat-A evidence: `.mq.md` print prototype

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0–P3 changes.
Decision record: [ADR-0027](../adr/0027-print-migration-compat-a.md)
Related: [Resource P3](2026-11-20-resource-stream-p3.md),
[file write](2026-11-20-resource-file-write-p3.md), [STATUS §5](../STATUS.md).
Scope: **Compat-A only** at the time of that slice. A later ADR-0030 step
added Compat-B routing; this report remains the Compat-A baseline.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  407 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Prototype

Ordinary `.mq.md` modules (now the bundled `stdlib/` tree, ADR-0028):

| File | Responsibility |
|---|---|
| `format.mq.md` | `显示` for Unit/None/Bool/Int/Float/Text/List/Map/Table, `值显示` (repr), `转义文本`, `列表显示`, `映射显示`, `表格显示`, `连接文本` |
| `text.mq.md` | `utf8编码(文本, 内存, 基址)` — 1–4 byte UTF-8 into `Memory` |
| `io.mq.md` | `写文本` (encode → `resource.write(stdout)` → close) and `写值` (format + newline) |
| `demo.mq.md` | side-by-side `io.写值(x)` and builtin `打印(x)` for a nested list and a table |

The call chain is exactly the RFC target shape:

```text
io.写值(value)
  -> format.显示(value)           # .mq.md value formatting, no Rust display()
  -> text.utf8编码(text, memory)  # .mq.md UTF-8 encoder
  -> resource.open(stdout)
  -> resource.write(memory, ...)  # bytes through Memory
  -> resource.close(stdout)
```

No `HostOp`, no new builtin, no parser change and no special stdlib dispatch.
The module is loaded only through an explicit Markdown import alias.

## Output comparison

```text
$ marqdo run stdlib/demo.mq.md
["文本", 2, 1.5, [3, true], {a: 1, b: "x"}, None]
["文本", 2, 1.5, [3, true], {a: 1, b: "x"}, None]
Table[名称 | 分数]{甲 | 87; 乙 | 92}
Table[名称 | 分数]{甲 | 87; 乙 | 92}
```

The first and third lines are `io.写值`; the second and fourth are builtin
`打印`. They are byte-identical.

CLI test:
`crates/cli/tests/examples_r6.rs::mqmd_stdlib_print_output_matches_builtin_print`
asserts exactly four lines, pairwise equality, and the exact list/table text.

## Covered and uncovered values

Covered by Compat-A comparison: `Unit`, `None`, `Bool`, `Int`, `Float`, `Text`,
`List`, `Map`, `Table`.

Not covered: `Formula`, `Code`, `FunctionRef`, `PluginHandle`. The language
still has no public observation for the module/name/handle identity needed to
reproduce `Value::display()`/`repr()` for those types; adding it is a CM-A04
extension, not part of this slice. This is the explicit blocker for Compat-B.

## What is not migrated

* At the time of this slice, `打印`/`print` resolved to the builtin and
  `HostOp::Print`; ADR-0030 later added Compat-B routing.
* No alias table, no bundled stdlib resolution, no `std` package.
* No stdlib version/digest or offline distribution (CM-A10).
* No Compat-B/C comparison lane or removal of the Rust `Value::display()` path.
* The demo/stdlib is an `examples/` prototype, not a shipped standard library.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo run -q -p marqdo-cli -- run stdlib/demo.mq.md
cargo test -p marqdo-cli --test examples_r6
cargo test --workspace --locked
```
