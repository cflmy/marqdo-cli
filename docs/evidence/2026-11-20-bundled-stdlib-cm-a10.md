# RFC-CM CM-A10 evidence: bundled stdlib, `std:` imports and digest

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `92bfbf2` plus this step's changes.
Decision record: [ADR-0028](../adr/0028-bundled-stdlib-and-std-imports.md)
Related: [Compat-A](2026-11-20-stdlib-print-compat-a.md), [STATUS §5](../STATUS.md).
Scope: bundled first slice; **no external repository publication**.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  415 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Bundle

`stdlib/` is the official offline bundle:

```text
stdlib/
  stdlib.json      # name, version 0.1.0, sha256 digest
  README.md
  format.mq.md     # 显示 / 列表显示 / 映射显示 / 表格显示 / 转义文本
  text.mq.md       # utf8编码 into Memory
  io.mq.md         # 写文本 / 写值 via resource.write(stdout)
  demo.mq.md       # builtin 打印 comparison
  tests/io.mq.md   # std:io self-test
```

Digest scheme: SHA-256 over sorted `*.mq.md` relative paths, NUL, file bytes,
NUL. `stdlib.json` and non-`.mq.md` files are excluded. The loader recomputes
the digest before loading any stdlib module and fails closed on mismatch or a
missing manifest.

`std:<relative>` is an explicit front-matter import target; it resolves only
inside the bundle, rejects `..`/absolute paths, appends `.mq.md` when absent,
and uses package instance
`std:marqdo-stdlib@0.1.0#<sha256-hex>`.

## Tests

`crates/cli/tests/stdlib_bundle.rs` (6):

```text
std_import_resolves_the_verified_bundle
  index.mq.md imports std:io; io.写值(["bundle",1,2.5])
  -> ["bundle", 1, 2.5]

analysis_records_the_verified_stdlib_digest
  check --emit contains project.stdlib.version/digest/instance_id
  matching stdlib.json

tampered_bundle_is_rejected_before_module_loading
  copy bundle, modify io.mq.md
  -> non-zero, "stdlib digest mismatch"

missing_bundle_manifest_is_a_hard_error
  empty MARQDO_STDLIB_DIR -> non-zero stdlib error

bundle_uses_no_foreign_or_plugin_calls
  scans all stdlib *.mq.md for foreign./plugin. -> none

frozen_binary_hash_is_unchanged_after_stdlib_execution
  sha256(marqdo binary) before == after running the std:io project
```

CLI unit tests in `crates/cli/src/stdlib.rs` (2): dev bundle resolves and
verifies; a copied/tampered bundle is rejected.

Existing `mqmd_stdlib_print_output_matches_builtin_print` still passes with the
demo moved to `stdlib/demo.mq.md`.

## Analysis identity

Analysis v2 project object now carries:

```json
{"stdlib":{"version":"0.1.0",
           "digest":"sha256:<64 hex>",
           "instance_id":"std:marqdo-stdlib@0.1.0#<64 hex>"}}
```

It is `null` when no bundle is available. The digest is verified before load,
so an Analysis artifact cannot name mismatched stdlib bytes.

## What is not done

* No separate `cflmy/marqdo-stdlib` repository, release tag, package registry
  or CI-signed digest. The bundle is vendored in this repository and copied
  next to the executable in a release layout by later packaging work.
* No CM-A07 Compat-B/C routing; `打印` is still the builtin default.
* No Windows/macOS bundle layout evidence and no remote CI URL.
* No independent stdlib upgrade workflow; changing a module requires updating
  `stdlib.json.digest`, and an incompatible core requirement is not yet
  enforced by a version table.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-cli --test stdlib_bundle
cargo run -q -p marqdo-cli -- run examples/stdlib-bundle.mq.md
cargo test --workspace --locked
```
