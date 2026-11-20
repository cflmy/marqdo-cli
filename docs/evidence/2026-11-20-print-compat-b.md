# RFC-CM CM-A07 Compat-B evidence: `打印` routing through the stdlib

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `e119727` plus this step's changes.
Decision record: [ADR-0030](../adr/0030-print-compat-b-routing.md)
Related: [Compat-A](2026-11-20-stdlib-print-compat-a.md),
[full-value formatting](2026-11-20-full-value-formatting.md),
[bundled stdlib](2026-11-20-bundled-stdlib-cm-a10.md).
Scope: Compat-B routing; **Compat-C removal is not done**.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  416 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Routing

* `Program.print_override: Option<FunctionId>` (IR, serde-optional).
* Loader scans loaded IR for bare `打印`/`print` calls. Only if found, it loads
  bundled `std:io` and sets the override to `io.写值`.
* Runtime resolution: local callable > print override > builtin table >
  same-module/imported function. Local shadowing is unchanged.
* The stdlib path formats all 15 value types (ADR-0029), UTF-8-encodes into
  `Memory`, opens `stdio/stdout`, writes and closes.
* Programs without print calls keep their original module graph.

## Trace behavior

`crates/cli/tests/print_routing.rs::print_is_routed_through_the_stdlib_and_not_hostop_print`:

```text
run index.mq.md --trace-file trace.json
stdout = ["x", 1]
trace operations include resource.write
trace operations do NOT include print
```

Resource open/read/write/close are now recorded as `effect` events by
`StdHost`, so trace consumers see the real byte path. No values or secrets are
recorded.

## Lazy injection

`stdlib_print_modules_are_injected_only_when_print_is_used`:

```text
plain.mq.md (no print) -> no std: modules
prints.mq.md (打印)     -> std: modules present
```

Both loads record `project.stdlib` identity because a verified bundle is
available; only the print-using program loads its modules. `check`'s
human-readable module count excludes `std:` modules so bundle internals do not
change the project count.

## Strict mode

Strict L1 already permits the console. Compat-B needs the same channel, so
strict now permits **only** `resource.open("stdio","stdout",["write"])` with the
console grant. File/network/process/device resource opens remain
`permission.denied`. Host test:

```text
strict_mode_allows_only_the_existing_stdio_console_channel
  stdio write -> "strict"
  file read   -> permission.denied
  socket read -> permission.denied
```

This is the existing console capability through the resource ABI, not a new OS
capability; CM-A11 fail-closed remains for all other providers.

## Compatibility

* All semantic goldens and CLI print tests pass byte-identically, including
  the full-value formatting demo.
* `HostOp::Print` remains implemented as the no-bundle fallback and for
  Compat-A comparison tests; Compat-C removal is a separate decision.
* Trace consumers that hard-code `operation == "print"` must accept
  `resource.*`; this is documented as a P3 migration.

## What is not done

* Compat-C: `HostOp::Print` and `Value::display()` formatting are still in the
  public builtin path for no-bundle installs.
* No stdlib version upgrade workflow and no external repository publication.
* No Windows/macOS evidence and no remote CI URL.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-cli --test print_routing
cargo test -p marqdo-cli --test analysis_trace
cargo test -p marqdo-host --test resource_streams
cargo test --workspace --locked
```
