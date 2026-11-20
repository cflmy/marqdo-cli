# RFC-CM CM-A07 Compat-C evidence: legacy print formatting removed

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `e26971f` plus this step's changes.
Decision record: [ADR-0031](../adr/0031-print-compat-c-remove-host-formatting.md)
Related: [Compat-A](2026-11-20-stdlib-print-compat-a.md),
[Compat-B](2026-11-20-print-compat-b.md),
[full-value formatting](2026-11-20-full-value-formatting.md).
Scope: Compat-C removal of the public host formatting path.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  419 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Removed

* `HostOp::Print` variant, its stable op name and `StdHost::op_print`.
* The print builtin's host dispatch. `打印/print` remains a builtin **name**
  for arity validation, but it no longer formats or writes:
  * with `Program.print_override` set (official CLI + bundle), the runtime calls
    `stdlib/io.写值`;
  * without an override, it returns `kernel.unavailable`:
    "打印 requires the bundled stdlib print module; install the official CLI
    bundle."

`Value::display()` remains for CLI result rendering/diagnostics but is not
reachable from the print builtin.

## Runtime/embedder behavior

`crates/marqdo-runtime/tests/interpreter.rs::print_without_the_bundled_stdlib_is_a_structured_error`
builds a Program without an override and asserts the structured error.

The semantic conformance harness builds Programs directly, so it now mirrors
the loader's Compat-B injection: it loads `stdlib/io.mq.md` (with
`format`/`text`), adds import edges and sets `print_override`. Existing print
fixtures 22/23/30/42 pass byte-identically.

## Trace and output

`crates/cli/tests/print_routing.rs` still asserts a print program's trace
contains `resource.write` and no `print` effect. The CLI loader routes official
runs through stdlib; `打印` output remains byte-identical, including the
full-value Formula/Code/FunctionRef demo.

## Compatibility notes

* Breaking for embedders that call `打印` without installing the stdlib print
  module; the official CLI loader does this automatically.
* `HostOp` loses a variant; any external exhaustive match must be updated.
  This is pre-1.0 and recorded in ADR-0031.
* No bundle means no print; this is fail-closed rather than a silent formatter.
* Compat-A/B evidence remains historical; their fallback wording is superseded.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-runtime --test interpreter print_without
cargo test -p marqdo-cli --test print_routing
cargo test -p marqdo-cli --test conformance_semantics
cargo test --workspace --locked
```
