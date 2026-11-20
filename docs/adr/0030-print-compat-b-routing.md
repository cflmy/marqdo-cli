# ADR-0030: Compat-B `打印` routing through the bundled stdlib

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A07, CM-A11, P3), ADR-0027, ADR-0028, ADR-0029
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-print-compat-b.md`

## Context

ADR-0027 implemented Compat-A: the stdlib print chain exists behind explicit
imports, but builtin `打印` still runs `Value::display()` in Rust. ADR-0029
removed the formatting gap: the stdlib can reproduce `打印` for all 15 value
types. ADR-0028 made a verified stdlib bundle available offline. The remaining
Compat-B requirement is to route the **same name** to one implementation and
prove the old callers behave, before Compat-C removes the legacy formatting
path.

## Decision

### 1. One program-level override

`Program` gains an optional `print_override: FunctionId`. The loader sets it to
the bundled `io.写值` function when:

* the verified bundle is available, and
* at least one loaded module calls `打印` or `print` as a bare builtin call.

The stdlib `io` module is injected only in that case. A program without print
calls keeps the same module graph and Analysis output as before. If no bundle
is available, the loader leaves the override unset and the legacy builtin is the
fallback; an official release always ships the bundle.

### 2. Resolution order

At a call site, name resolution is:

1. a local variable holding a callable (`f = ...; f(x)`) — local shadow wins;
2. `打印`/`print` with `Program.print_override` set — call the stdlib function;
3. the existing builtin table;
4. a same-module/imported function declaration.

This is a controlled compatibility route for exactly two names. It is not a
general name-shadowing mechanism and does not add a new namespace.

### 3. Runtime behavior

The stdlib function is a normal Marqdo function: its source spans, fuel,
frames and trace are the stdlib file's. It formats through
`stdlib/format.mq.md`, encodes UTF-8 through `stdlib/text.mq.md`, and writes
through `resource.open("stdio","stdout",["write"])` plus `resource.write`.
`打印` still returns `Unit` and appends exactly one newline.

The console/output-budget checks remain in the host: the stdlib path uses the
same console grant and output budget as the legacy path. Resource operations
are recorded as Trace `effect` events (`resource.open/read/write/close`); the
legacy `HostOp::Print` effect does not appear in Compat-B runs.

### 4. Strict mode and CM-A11

Strict L1 already permits the console channel. Compat-B needs stdout writes in
strict so the same print implementation works there. Therefore strict mode
permits **only** `stdio/stdout` write with the console grant; project-file,
network, process and device resource opens remain refused, and no new provider
is enabled. This is the existing console capability reached through the
resource ABI, not a new OS capability. File resources stay fail-closed.

### 5. Compatibility and fallback

* Output for all 15 value types is the ADR-0029 formatter and is
  comparison-tested against the legacy path.
* Trace shape changes from one `print` effect to resource effects. Consumers
  that hard-code `operation == "print"` must accept `resource.*`; this is a
  documented P3 migration.
* `check`/`analysis` include stdlib modules for print-using programs only.
  The human-readable `checked N module(s)` count excludes stdlib modules so
  bundle internals do not change the project count.
* Compat-C (removing `HostOp::Print` and `Value::display()` formatting from the
  public builtin) is **not** part of this ADR; it is implemented by ADR-0031,
  which supersedes the "legacy fallback" sentence above.

## Non-goals

* No routing for `长度`, `类型`, `文本`, table or any other builtin.
* No bundled stdlib upgrade workflow; the bundle version stays `0.1.0`.
* No removal of `HostOp::Print`, `Value::display()` or the legacy builtin.
* No new OS isolation claim; strict still refuses file/network/process
  resources.
* No guarantee for a CLI installed without `stdlib/`; that configuration keeps
  legacy print and is not the official layout.

## Verification

`docs/evidence/2026-11-20-print-compat-b.md` records:

* a print program whose trace contains `resource.write` and no `print` effect;
* all semantic goldens and CLI print tests passing byte-identically;
* lazy stdlib injection: no print → no stdlib modules; print → stdlib modules
  and `project.stdlib` identity;
* host tests for strict stdio-only permission and file refusal.
