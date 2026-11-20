# ADR-0027: CM-A07 print-migration staging and the Compat-A `.mq.md` stdlib prototype

Status: **Accepted (Compat-A only; Compat-B/C pending)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A07, CM-A10, P3), ADR-0023, ADR-0024, ADR-0025, ADR-0026
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-20-stdlib-print-compat-a.md`

## Context

RFC-CM-0002 §5.4 and CM-A07 require `打印` to stop being a hidden Rust
`Value::display()` path. The migration has three compatibility stages:

* **Compat-A**: a real `.mq.md` print implementation exists and is callable
  through explicit imports; the legacy builtin remains the default.
* **Compat-B**: the `print/打印` name routes to the stdlib implementation
  (or an alias table maps it), while old behavior is comparison-tested.
* **Compat-C**: the legacy `HostOp::Print` formatting path is removed from the
  public builtin implementation.

This ADR freezes the staging policy and implements **Compat-A only**. It does
not change `打印` resolution, does not remove `HostOp::Print`, and does not
claim CM-A07 complete.

## Decision

### 1. Stage policy

* A stage is only complete when its comparison evidence exists:
  * A: explicit stdlib functions exist and their output is compared with the
    builtin for the covered value set;
  * B: name resolution tests prove a single implementation is selected and old
    callers still behave;
  * C: all old goldens pass with no `HostOp::Print` formatting path.
* Stages are additive; no stage may silently make a second implementation
  reachable for the same name.
* `strict` behavior is unchanged by stdlib-only code; a stdlib print still
  needs the console/resource grant.

### 2. Compat-A implementation

Add a normal `.mq.md` module package (later moved to the bundled `stdlib/` tree by ADR-0028):

```text
stdlib/format.mq.md   # 显示 / 列表显示 / 映射显示 / 表格显示 / 转义文本
stdlib/text.mq.md     # utf8编码 (Memory bytes)
stdlib/io.mq.md       # 写文本 / 写值 (resource.open stdout + write/close)
stdlib/demo.mq.md     # side-by-side comparison with 打印
```

`io.写值(值)` = `format.显示(值) + "\n"` → `text.utf8编码` → `Memory` →
`resource.open("stdio","stdout",["write"])` → `resource.write` → `close`.
This uses only the public core/resource contracts; it is ordinary imported
`.mq.md` code, not a new builtin.

Covered value types: `Unit`, `None`, `Bool`, `Int`, `Float`, `Text`, `List`,
`Map`, `Table`. Formatting matches the builtin `Value::display()` for the
checked corpus, including Text quoting/escaping inside collections, canonical
map key order and table layout.

**Not covered yet**: `Formula`, `Code`, `FunctionRef`, `PluginHandle`. The
language currently has no public observation for the module/name/handle
identity needed to reproduce `Value::display()`/`repr()` for those types.
Adding such observation would be a CM-A04 extension and is out of scope.

### 3. Compatibility rules

* `打印` remains the default builtin in this slice. `io.写值` is only reachable
  through an explicit import alias; there is no implicit module, bundled stdlib
  resolution or global name shadowing.
* No `HostOp` is added or removed. Both paths write through the console/output
  budget; the stdlib path additionally goes through the resource ABI.
* Stdlib modules are ordinary Markdown documents: no special parser, no
  embedded Rust formatter, no direct formatting syscall.
* A future Compat-B ADR must define name precedence, bundled module lookup,
  stdlib version/digest (CM-A10) and failure behavior; this ADR does not.

### 4. Non-goals

* No `print` alias routing, no removal of `HostOp::Print`.
* No bundled stdlib install/version/digest or offline distribution.
* No formatting of Formula/Code/FunctionRef/PluginHandle.
* No locale-aware, width-aware or float-format changes.
* No change to Markdown rendering, `strict`, or the wire/ABI.
* No claim that the examples directory is the shipped standard library.

## Consequences

* The target call chain (`format.display` → `utf8_encode` → `write_all` →
  `resource.write`) is exercised end to end in `.mq.md` and can be compared to
  the builtin.
* The missing observation for `Formula/Code/FunctionRef/PluginHandle` is now an
  explicit, documented blocker for Compat-B rather than an implicit assumption.
* Because the stdlib is not bundled, ordinary users still get the builtin; the
  prototype does not yet reduce the runtime's formatting surface.

## Verification

`docs/evidence/2026-11-20-stdlib-print-compat-a.md` records the side-by-side
demo (`io.写值` vs `打印` for a nested list and a table), the CLI test asserting
byte-identical lines, and the full workspace gate. It also records the covered
and uncovered value types.
