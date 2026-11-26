# Issue #11 Phase A — one explicit stdlib Bundle input

Date: 2026-11-26.
Implementation baseline: `4bbefc92449ee480c8a2c89b0308feb4cda8d0e7`.
Bundle source commit: `2cbde54602c07f62a62f341400bc9cb4764a93f8`.
Decision: [ADR-0070](../adr/0070-explicit-stdlib-bundle-build-input.md).
Inventory:
[`2026-11-26-stdlib-source-coupling-inventory.md`](2026-11-26-stdlib-source-coupling-inventory.md).

Candidate commits (in order):

| Commit | Change |
|---|---|
| `b9c72e1` | ADR-0070 + source-coupling inventory |
| `1b9d9c4` | `tools/stdlib/bundle.py`, `bundle-pin.json`, required `pack-stdlib.py --stdlib-dir` |
| `422b9de` | `package.py --stdlib-dir/--bundle-pin`, `RELEASE_INFO.stdlib.source` |
| `3a68260` | pinned test resolver, synthetic loader tests, product/conformance rewiring |
| `ab6b6b1` | independent-build lane, perf Bundle pin, CI and docs |
| `0e1efc7` | archive smoke/cold-install keep the language-only run after removing the Bundle |

The documentation commit that adds this record also updates `STATUS.md`, the
architecture overview, `README.md` and `tests/README.md`. CI follow-up commit
`2acaaef` folds the `independent-build` job into the existing `linux` evidence
gate (the release-evidence schema accepts only its declared gate names).

Executed host: Linux x86_64 (glibc 2.43), `rustc 1.100.0 (0ef1bc386
2026-11-09)`, `cargo 1.100.0 (3acc79c0c 2026-11-05)`, Python 3.14.4; the
cold-install container is `ubuntu:24.04` (glibc 2.39) with `--network none`.

## 1. Pinned Bundle identity

```console
$ python3 tools/stdlib/bundle.py
{
  "aliases_digest": "sha256:5f513c0fdbb3c1e9829be3c9fa23a2d1e89cb8510073fb5eb1e8e2f5265a83bb",
  "file_count": 42,
  "name": "marqdo-stdlib",
  "root": "<checkout>/stdlib",
  "source_commit": "2cbde54602c07f62a62f341400bc9cb4764a93f8",
  "tree_digest": "sha256:5407d3665ba2fe105820d0f1af5efaafa10c277a69eb819c3225719813c8c713",
  "version": "0.1.0"
}
```

`tools/stdlib/bundle-pin.json` records the same identity. The resolver accepts
an explicit location but never an explicit identity: a valid-but-unpinned
Bundle is rejected.

## 2. Failure paths (tools/ci/test_stdlib_bundle_tools.py)

64 `tools/ci` unit tests pass. The new cases cover:

* `--stdlib-dir` is required and a missing input fails before any output;
* a missing `stdlib.json`, a tree-digest mismatch, an alias-digest mismatch and
  `public_aliases` without `aliases_digest` fail;
* a symlink inside the Bundle is rejected;
* an explicit Bundle that does not match the pin fails, even when the pinned
  default is valid;
* a bad pin schema fails;
* the `bundle.py` CLI exits 1 on an invalid explicit input and 0 with
  `--print-path` otherwise.

The runtime override rule is pinned by the loader unit test
`bad_override_fails_closed_even_with_a_valid_sibling` (a bad
`MARQDO_STDLIB_DIR` fails although a valid sibling `stdlib/` exists).

## 3. Deterministic packaging

Two standalone stdlib packs from the same explicit input and toolchain are
byte-identical:

```console
$ python3 tools/pack-stdlib.py --stdlib-dir stdlib \
    --bundle-pin tools/stdlib/bundle-pin.json --out-dir <a>
$ python3 tools/pack-stdlib.py --stdlib-dir stdlib \
    --bundle-pin tools/stdlib/bundle-pin.json --out-dir <b>
$ sha256sum <a>/marqdo-stdlib-0.1.0.tar.gz <b>/marqdo-stdlib-0.1.0.tar.gz
f6dc8e7ad92bc58b815635aea923069919d6f5495078e53828ae4723d925a565  <a>/marqdo-stdlib-0.1.0.tar.gz
f6dc8e7ad92bc58b815635aea923069919d6f5495078e53828ae4723d925a565  <b>/marqdo-stdlib-0.1.0.tar.gz
```

Two release archives built from `ab6b6b1` with `--skip-example-build` (the
host-built example artifacts were already present) are byte-identical:
`f37e01c44e668a6daf0538b32e9825cc74392f0ccbd65362af2783629b6d2c48`.
`RELEASE_INFO.json.stdlib.source` records
`{"kind":"in-repo","repository":"cflmy/marqdo-cli","commit":"2cbde54602c07f62a62f341400bc9cb4764a93f8","path":"stdlib"}`
and the tree/alias digests equal `stdlib.json`.

## 4. Product and performance lanes

The product, HTTP, table, alias-routing, P2 closure and semantic harnesses
resolve the pinned Bundle through `crates/cli/tests/support/stdlib.rs`
(`MARQDO_TEST_STDLIB_BUNDLE`/`MARQDO_TEST_BUNDLE_PIN` overrides are verified
against the pin). Loader unit tests build synthetic Bundles instead of the
product tree.

```console
$ cargo test -p marqdo-cli --bin marqdo stdlib --offline
4 passed (synthetic loader tests)
$ cargo test -p marqdo-cli --test stdlib_product --offline
16 passed
$ cargo test -p marqdo-cli --test stdlib_bundle --test stdlib_http \
    --test stdlib_table --test stdlib_alias_routing --offline
13 passed
$ cargo test -p marqdo-cli --test architecture_boundaries \
    --test conformance_semantics --test p2_closure --offline
16 passed
$ python3 tools/perf/runner.py --no-build --binary target/debug/marqdo \
    --warmups 1 --samples 3 --allow-smoke --only hello-check ...
correctness=VERIFIED status=SMOKE
stdlib={... "source_commit":"2cbde54602c07f62a62f341400bc9cb4764a93f8" ...}
```

## 5. Independent build (clean copy without stdlib)

`tools/ci/independent_build.py` exported `ab6b6b1` with `git archive`, removed
`stdlib/`, and built both binaries with `--locked`:

```json
{
  "status": "PASS",
  "source_sha": "ab6b6b1a23ec064fbc7de943c52befca18142a56",
  "binaries": {
    "marqdo": {"sha256": "428c25c60caa7762348bcffad96558591adf52440d44549a645390c927558757"},
    "marqdo-provider-worker": {"sha256": "3787942de6d35ce1c7f03c0848c8e08b6037495e98005c62704d34e664da795c"}
  },
  "checks": {
    "stdlib_source_removed": true,
    "language_only_check": "PASS",
    "language_only_call": "PASS:3",
    "stdlib_dependent_missing_bundle": "PASS",
    "no_sibling_stdlib": true
  }
}
```

The stdlib-dependent `run examples/hello.mq.md` failed with an explicit
stdlib/print diagnostic instead of falling back to a source tree; the built
`target/debug` directory contains no `stdlib/`.

## 6. Archive smoke and cold install

```console
$ python3 tools/release/smoke.py --archive <archive>
smoke: verified stdlib 0.1.0 tree=sha256:5407d36... aliases=sha256:5f513c0... files=42
smoke: archive version 3.0.0-rc.4, source ab6b6b1a23ec064fbc7de943c52befca18142a56
smoke: bundled stdlib missing -> fail-closed OK
smoke: language-only program runs without the bundled stdlib
smoke: PASS
```

```console
$ bash tools/release/cold-install.sh <archive> ab6b6b1a23ec064fbc7de943c52befca18142a56
你好，Marqdo
checked 2 module(s): ok
checked 1 module(s): ok
6
runtime error[kernel.unavailable]: 打印 requires the bundled stdlib print module; install the official CLI bundle
<language-only factorial call still prints 120>
```

The container is `ubuntu:24.04`, `--network none`, user `65534:65534`, with no
Cargo and no source checkout; only the archive bytes are mounted. Moving the
shipped `stdlib/` aside keeps the language-only factorial call working and
makes the `打印` program fail with the explicit `kernel.unavailable` message.

## 7. Gates on the candidate tree

```console
$ cargo fmt --all -- --check
FMT_OK
$ cargo clippy --workspace --all-targets --offline -- -D warnings
exit=0
$ cargo test --workspace --locked --offline
exit=0
634 passed, 0 failed, 2 ignored across 72 test binaries
```

The workspace run includes `docs_consistency` (7 passed),
`architecture_boundaries`, the frozen conformance suites and every product
lane. It was executed on the candidate tree plus this documentation change and
is reproducible with `cargo test --workspace --locked`. Platform-native
Windows/macOS runs were not re-executed for this docs-only candidate; the
existing GitHub matrix remains the lane for them.

## 8. Not claimed

* Phase B: switching to the ecosystem-assembled Bundle, removing the in-repo
  production sources, removing the debug source fallback and deleting replaced
  `std:` entries. It is gated on ecosystem issue #2 and the extension
  replacement evidence named by issue #11.
* Any `std:`/`stdlib.json`/`marqdo.stdlib-package`/Analysis/Observation shape
  change; none was made.
* A docker cold-install on a Windows/macOS host or a native Windows/macOS
  product run for this candidate.
