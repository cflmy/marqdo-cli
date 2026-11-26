# ADR-0070 — One explicit stdlib Bundle build input (Phase A)

Status: **Accepted for implementation on the 3.0 prerelease line** (2026-11-26).
Relates to: issue
[#11](https://git.cflmy.cn/cflmy/marqdo-cli/issues/11), ADR-0028 (bundled
stdlib and `std:` imports), ADR-0039 (deferred extraction), ADR-0058 (public
aliases), ADR-0062 (resource ownership), ADR-0068 (single current public
contracts).
Evidence:
[`docs/evidence/2026-11-26-stdlib-source-coupling-inventory.md`](../evidence/2026-11-26-stdlib-source-coupling-inventory.md).

## Context

Issue #11 starts the extraction that ADR-0039 deferred: the CLI and the
Provider Worker must build and run on their own, while a release is assembled
from one explicitly provided, verifiable stdlib Bundle. The extraction trigger
listed by ADR-0039 is now real: an ecosystem repository pins component commits
and assembles the production Bundle, and nine component repositories own module
sources.

At the implementation baseline `4bbefc92449ee480c8a2c89b0308feb4cda8d0e7` the
stdlib source directory is implicit in four different ways, documented in the
inventory linked above:

* both packaging entry points read the checkout's `stdlib/` with no build
  argument (`tools/pack-stdlib.py`, `tools/release/package.py`);
* product tests, the semantic harness and the P2 closure lane rebuild
  `repo_root()/stdlib` independently;
* the performance runner silently falls back to `ROOT / "stdlib"` when the
  binary has no sibling bundle;
* the debug CLI resolves a compile-time source tree
  (`CARGO_MANIFEST_DIR/../../stdlib`) after the explicit override and the
  sibling directory.

Separating compile, package and product-test lanes later requires changing all
of them at once, and none of them records which Bundle commit produced a test
run. The runtime contracts themselves — `stdlib.json`, the
`marqdo.stdlib-package` descriptor, Analysis `project.stdlib` and Observation
identity — are not the problem and must not change.

## Decision

### 1. Explicit build input

`tools/pack-stdlib.py` and `tools/release/package.py` accept the same required
`--stdlib-dir PATH` argument. It names an unpacked Bundle root that contains
`stdlib.json`. There is no implicit default: a missing path, a missing manifest,
a tree-digest mismatch, an alias-digest mismatch or a missing required module
fails the command before any archive is written. Both entry points keep the
existing deterministic archive layout, descriptor fields, digest algorithms
and checksums.

Both entry points also accept an optional `--bundle-pin PATH`. When given, the
input must match the pinned identity and the release record names the pinned
source commit; the pin is not a substitute for verifying the Bundle's own
manifest.

### 2. One pinned Bundle identity

`tools/stdlib/bundle-pin.json` records the Bundle name, version, source
repository and commit, tree digest and alias digest. `tools/stdlib/bundle.py`
is the single resolver and verifier for source-side consumers: tests, the
performance runner and release CI resolve the Bundle through it instead of
concatenating `stdlib/`.

An explicit `MARQDO_TEST_STDLIB_BUNDLE` (tests) or `--stdlib-dir` (tools) takes
precedence over the pin's default location, but the input must still verify
against the pin's digests. An invalid explicit input fails even when a valid
sibling Bundle exists; silent fallback is forbidden. Phase B updates the pin to
the Bundle assembled by the ecosystem repository; the algorithm does not change.

### 3. Runtime contract unchanged

`MARQDO_STDLIB_DIR` remains the explicit runtime override (invalid values fail
hard), followed by `stdlib/` next to the executable. The debug-only
compile-time source fallback remains in Phase A so the development loop keeps
working; it is removed in Phase B together with the in-repo production sources.
`stdlib.json`, `marqdo.stdlib-package`, Analysis `project.stdlib`, module ids
and Observation identity keep their current shapes and values.

### 4. Separated test lanes

The lanes become explicit:

* **independent compilation** — `cargo build --locked -p marqdo-cli
  -p marqdo-provider-worker` in a copy without `stdlib/`, then a language-only
  program through the real CLI;
* **loader unit tests** — small synthetic bundles built in a temporary
  directory, so loader logic does not depend on the product tree;
* **product tests** — the whole pinned Bundle through `std:` and the real CLI;
* **conformance** — the frozen, independent fixture corpus, unchanged.

### 5. Phase B gate

Phase B (switch to the ecosystem-assembled Bundle, delete the in-repo
production sources, the debug source fallback and the replaced `std:` entries)
requires the accepted component commits of ecosystem issue #2 and the extension
replacement evidence named by issue #11. This ADR records the Phase A input
boundary only; it does not authorize deleting sources or changing `std:`
semantics.

## Consequences

* A packaging run can be pointed at a downloaded or separately built Bundle and
  still produce the same deterministic archive and `RELEASE_INFO.json` record.
* Every product/perf CI run names the Bundle commit and digest it used; a
  stale Bundle is visible instead of implicit.
* Phase B becomes a pin/CI change plus source removal, not a simultaneous
  rewrite of the test lanes.
* The repository still carries the Bundle in Phase A, so the extraction is not
  claimed as complete. `docs/STATUS.md` must record the Phase A state, not the
  final externalized state.

## Non-goals

* Changing `std:` spelling, digest algorithms, module ids, public aliases or
  the `stdlib.json`/descriptor shapes.
* Fetching a Bundle over the network, adding a package registry or a signature
  scheme.
* Removing the debug source fallback, the in-repo `stdlib/` sources or any
  `std:` entry in Phase A.
* Declaring the extraction, the external Conformance suite or the ecosystem
  switch done before their evidence exists.
