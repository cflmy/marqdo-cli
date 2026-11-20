# ADR-0031: Compat-C — remove `HostOp::Print` and the legacy formatting path

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A07, P3), ADR-0027, ADR-0029, ADR-0030
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-print-compat-c.md`

## Context

Compat-A produced a stdlib print chain; Compat-B (ADR-0030) routed
`打印/print` to the bundled `io.写值` and proved output parity for all 15 value
types. The legacy path (`HostOp::Print` → `Value::display()` → output buffer)
remained as a no-bundle fallback and a comparison reference. Compat-C removes
the **public builtin formatting path** so a print call can no longer silently
fall back to a Rust formatter.

## Decision

### 1. Remove the host operation

* `HostOp::Print` and `StdHost::op_print` are deleted. There is no host
  operation that formats a value.
* `打印/print` remains a **builtin name** for static arity validation and
  compatibility diagnostics. Its implementation no longer writes anything; if
  the runtime reaches it (no `Program.print_override`), it returns the
  structured `kernel.unavailable` error:
  "打印 requires the bundled stdlib print module; install the official CLI
  bundle."
* The official CLI ships the verified bundle, so a print-using program always
  loads `stdlib/io.mq.md`, sets `print_override`, and executes `io.写值`.
  A CLI installed without `stdlib/` cannot print; that layout is not official.

### 2. Formatting ownership

All print formatting, UTF-8 encoding and byte output now live in `.mq.md`:

```text
打印(value)
  -> stdlib/io.写值
  -> stdlib/format.显示
  -> stdlib/text.utf8编码
  -> resource.open("stdio","stdout",["write"])
  -> resource.write
  -> resource.close
```

`Value::display()` still exists for CLI result rendering (`call --json`,
diagnostics), but it is **not** reachable from the print builtin. No `HostOp`
accepts a value for display.

### 3. Trace and budgets

Print produces only resource effects (`resource.open/write/close`) and stdlib
semantic events. There is no `print` effect. Output budget and console grant
are enforced at the resource/stdio boundary exactly as before.

### 4. Compatibility

* All semantic goldens, CLI print tests and the full-value formatting demo pass
  unchanged.
* The runtime embedding API (`marqdo_runtime::run`) no longer prints unless the
  caller installs a `Program.print_override`; a direct runtime program that
  calls `打印` gets the structured no-bundle error. This is a **breaking change
  for embedders** and is recorded here; the CLI loader maintains the official
  behavior.
* `callable.rs` still advertises the builtin name for diagnostics and arity;
  this is not a second implementation.
* Compat-A/B evidence remains as history; its "legacy fallback" wording is
  superseded by this ADR.

### 5. Non-goals

* Removing the `打印/print` builtin name, alias, or arity validation.
* Removing `Value::display()` for CLI result output, diagnostics or format
  helpers.
* Supporting print without the bundle; no implicit formatting fallback.
* Changing Resource ABI, strict mode or stdlib version.

## Consequences

* It is now impossible for `打印` to execute a hidden Rust formatter; the only
  print implementation on an official installation is `.mq.md`.
* Embedders that build a `Program` directly must load the stdlib print module
  and set `print_override` (the semantic test harness does this).
* Trace consumers see resource effects, not a print effect.
* A missing/corrupt bundle fails closed for print instead of silently using a
  different implementation.

## Verification

`docs/evidence/2026-11-20-print-compat-c.md` records: no `HostOp::Print` or
`op_print` references; the no-bundle structured error; trace still shows
resource effects; all 419 tests and the full-value demo pass.
