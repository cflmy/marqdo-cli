# Stdlib source-directory coupling inventory (issue #11, Phase A)

Date: 2026-11-26.
Implementation baseline: `4bbefc92449ee480c8a2c89b0308feb4cda8d0e7` (current
`master`). Bundle source commit: `2cbde54602c07f62a62f341400bc9cb4764a93f8`
(the last commit that touched `stdlib/` at the baseline). Pinned Bundle:
`marqdo-stdlib` `0.1.0`, tree digest
`sha256:5407d3665ba2fe105820d0f1af5efaafa10c277a69eb819c3225719813c8c713`,
alias digest
`sha256:5f513c0fdbb3c1e9829be3c9fa23a2d1e89cb8510073fb5eb1e8e2f5265a83bb`.

This file inventories every direct reference to the checkout's stdlib source
directory found for issue #11 Phase A, before the explicit build-input change.
It is an inventory, not a claim that Phase A is finished; the executed evidence
is recorded separately when the work lands. Decision:
[ADR-0070](../adr/0070-explicit-stdlib-bundle-build-input.md).

## Packaging and release tooling

| Entry point | Reference | Phase A disposition |
|---|---|---|
| `tools/pack-stdlib.py` | `STDLIB = ROOT / "stdlib"` (line 17); used by the tree digest, manifest read, file list, archive and descriptor (lines 37–97) | required `--stdlib-dir`, optional `--bundle-pin`; shared digest/verify helper |
| `tools/release/package.py` | `STDLIB = ROOT / "stdlib"` (line 37); `verify_stdlib` (lines 110–141), staging copy (line 229), `RELEASE_INFO.stdlib` (lines 245–264) | required `--stdlib-dir`, optional `--bundle-pin`; `RELEASE_INFO.stdlib.source` records the pinned commit |
| `tools/release/smoke.py` | reads only the extracted archive's `install/stdlib` (lines 81–116) | unchanged; it stays an independent verifier of the shipped bytes |
| `tools/perf/runner.py` | pops `MARQDO_STDLIB_DIR`, then falls back to `ROOT / "stdlib"` when the binary has no sibling bundle (lines 327–330) | resolve the pinned Bundle explicitly and always export `MARQDO_STDLIB_DIR`; no silent repo fallback |
| `tools/ci/test_release_archive.py` | reads `ROOT/stdlib/stdlib.json` and `ROOT/stdlib` (lines 33, 40, 54) | pass the explicit/pinned path; add failure-path assertions |
| `.github/workflows/ci.yml` | `package.py` invocation without a Bundle argument (line 224); product lane (lines 44–45) | pass `--stdlib-dir` + `--bundle-pin`; verify the pin in the product lane |
| `.woodpecker/main.yml` | product lane (line 51), `package.py` invocation (line 80) | same as above |
| `.woodpecker/performance.yml` | `package.py` invocation (line 25) | pass `--stdlib-dir` + `--bundle-pin`; perf runner resolves the pin |
| `docs/releasing.md` | release command without a Bundle argument (line 119) | document the required input and the pin |

## Runtime and development discovery

| Entry point | Reference | Phase A disposition |
|---|---|---|
| `crates/cli/src/stdlib.rs` | debug-only `CARGO_MANIFEST_DIR/../../stdlib` fallback after `MARQDO_STDLIB_DIR` and the sibling directory (lines 73–82) | keep for the development loop; remove in Phase B |
| `crates/cli/src/stdlib.rs` unit tests | `dev_bundle()` loads the whole in-repo tree (lines 457–492) | replace with synthetic small bundles so loader tests do not depend on the product tree |

## Test harnesses that rebuild `repo_root()/stdlib`

| Harness | Reference | Role | Phase A disposition |
|---|---|---|---|
| `crates/cli/tests/stdlib_product.rs` | `stdlib_dir()` (lines 35–37), fixtures (line 48), cwd/`MARQDO_STDLIB_DIR` (lines 130–138) | product | pinned resolver |
| `crates/cli/tests/stdlib_bundle.rs` | `stdlib_dir()` (lines 19–20) | product | pinned resolver |
| `crates/cli/tests/stdlib_http.rs` | cwd and `MARQDO_STDLIB_DIR` (lines 22–23) | product | pinned resolver |
| `crates/cli/tests/stdlib_table.rs` | cwd and `MARQDO_STDLIB_DIR` (lines 21–22) | product | pinned resolver |
| `crates/cli/tests/stdlib_alias_routing.rs` | copies `repo()/stdlib` (lines 64–65) | product | pinned resolver |
| `crates/cli/tests/p2_closure.rs` | table self-test path, cwd and env (lines 112, 122–123) | product | pinned resolver |
| `crates/cli/tests/conformance_semantics.rs` | alias binding (line 294), entry `std:` imports (line 431) | conformance | pinned resolver |
| `crates/cli/tests/architecture_boundaries.rs` | in-repo digest/import gates (lines 90–114, 442–477) | architecture gate | pinned resolver plus the Phase A in-repo assertion |

## References that are intentionally not build inputs

* `crates/cli/src/analysis/*`, `crates/cli/src/loader.rs` and
  `crates/cli/src/commands.rs` consume the verified `StdlibBundle` identity at
  run time; they do not read the source directory.
* `crates/marqdo-runtime/src/builtins_impl.rs` and
  `crates/marqdo-sandbox/*` mention stdlib only in diagnostics/tests.
* `docs/evidence/*` and historical ADRs keep their recorded paths; they are
  decision history, not executable inputs.

## Acceptance for Phase A

1. A clean copy without `stdlib/` and without an ecosystem checkout builds
   `marqdo-cli` and `marqdo-provider-worker` with `--locked`.
2. Both packaging entries fail without `--stdlib-dir`, and fail on a missing
   directory, a missing `stdlib.json`, a tree-digest mismatch, an alias-digest
   mismatch or a pin mismatch.
3. Product tests, the performance runner and release CI resolve the Bundle
   through `tools/stdlib/bundle.py` and name its pinned commit and digest.
4. The frozen Conformance corpus and the independent `tools/release/smoke.py`
   checks stay in place.
