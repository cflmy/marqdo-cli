# RFC-CM ADR-0029 evidence: full-value stdlib formatting

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `1377af1` plus this step's changes.
Decision record: [ADR-0029](../adr/0029-full-value-formatting-and-handle-observation.md)
Related: [Compat-A](2026-11-20-stdlib-print-compat-a.md),
[bundled stdlib](2026-11-20-bundled-stdlib-cm-a10.md), [STATUS §5](../STATUS.md).
Scope: observation members + stdlib formatter. No print name routing.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  416 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Observation members

`Value::FunctionRef` now exposes `module` and `name`; `Value::PluginHandle`
exposes `plugin` and `export`. The numeric host `PluginHandle.id` remains
private. Formula/Code members were already public. Memory/Resource remain
opaque with no fields.

Runtime test `function_ref_and_resource_members_are_readable` checks:

```text
ref("#目标").module  -> root:default::index.mq.md
ref("#目标").name    -> 目标
ref("#公式").latex   -> a = 1
ref("#公式").display -> true
ref("#代码").language-> text
ref("#代码").source  -> "x\n"
```

## Formatter coverage

`stdlib/format.mq.md` now has branches for all 15 value types:

`Unit`, `None`, `Bool`, `Int`, `Float`, `Text`, `List`, `Map`, `Table`,
`Formula`, `Code`, `FunctionRef`, `PluginHandle`, `Memory`, `Resource`.

* Formula/Code/FunctionRef/PluginHandle reproduce the existing `Value::repr()`
  spellings (not a new format).
* Code byte length is computed from `text.codepoints` by summing UTF-8 widths,
  not from `长度` (scalar count) and without a temporary `Memory`.
* Text inside collections is quoted/escaped; top-level Text is raw.

## Comparison

`stdlib/demo.mq.md` compares `io.写值` with builtin `打印`:

```text
["文本", 2, 1.5, [3, true], {a: 1, b: "x"}, None]
["文本", 2, 1.5, [3, true], {a: 1, b: "x"}, None]
Table[名称 | 分数]{甲 | 87; 乙 | 92}
Table[名称 | 分数]{甲 | 87; 乙 | 92}
[Formula("a = 1"), Code("text", 9 bytes), FunctionRef(root:default::demo.mq.md::演示函数)]
[Formula("a = 1"), Code("text", 9 bytes), FunctionRef(root:default::demo.mq.md::演示函数)]
```

`examples_r6::mqmd_stdlib_print_output_matches_builtin_print` asserts all three
pairs byte-identically, including the Formula/Code/FunctionRef line.

## What is not done

* No `打印/print` name routing (Compat-B). Builtin remains the default.
* PluginHandle formatting is covered by code branches and observation, but no
  fixture constructs a live PluginHandle without a plugin invocation.
* Control characters outside `"`, `\`, newline, tab and CR are not emitted as
  `\u00XX`.
* Memory/Resource formatting is the literal type name, matching
  `Value::display()`; no internal identity leaks.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo run -q -p marqdo-cli -- run stdlib/demo.mq.md
cargo test -p marqdo-cli --test examples_r6
cargo test -p marqdo-runtime --test interpreter function_ref_and_resource_members
cargo test --workspace --locked
```
