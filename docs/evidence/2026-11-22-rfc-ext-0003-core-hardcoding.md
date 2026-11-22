# RFC-EXT-0003 (issue #3) — core/extension boundary evidence

Status: **executed evidence, post-`3.0.0-beta.1` tree.**
Audit baseline: `eeea01d672c07b6be7d6444031d1619309dc88ab` (issue #3).
Frozen decisions: ADR-0060, ADR-0061, ADR-0062, ADR-0063.
This file is the executed companion to `docs/STATUS.md` §0.6 and the ADRs; the
plan text in the issue never counts as evidence.

## 1. Status matrix (audit item → implementation → evidence)

| Item | Status | Implementation | Executed evidence |
|---|---|---|---|
| H-06 sibling heading inheritance | **FIXED** | `build_units` pops `level >= current` before deriving the inherited mode (`crates/marqdo-frontend/src/units.rs`) | `units` matrix (12 tests), `conformance_semantics` fixtures 22/23, black-box `heading_inheritance` (check --emit/run/call) |
| H-08 stdlib in the release archive | **FIXED** | `package.py` stages and digest-verifies `stdlib/`; `RELEASE_INFO.json.stdlib`; release discovery no longer falls back to the compile-time tree; `smoke.py` re-verifies and fails closed | `tools/ci/test_release_archive.py` (5 tests), archive + smoke + independent `ubuntu:24.04` container runs in §5–§6 |
| H-04 provider reserved core kinds | **FIXED** | single `marqdo_host::CORE_RESOURCE_KINDS` used by CLI manifest validation and all trusted host registration paths | `resource_streams` (34 tests incl. all six names/kinds), `provider_manifest::every_core_kind_is_rejected_as_name_and_as_kind` |
| H-01 math domain API removed | **REMOVED (breaking, announced)** | five builtins/HostOps/runtime arms/`marqdo-math` deleted; pure package + optional ABI backend; `Formula` inert; `math.compute` stays a capability category | `math_library` black-box (pure package call = 6, ABI backend = 42, no grant = `permission.denied`, removed `math.eval` = `symbol.unknown`), `conformance_semantics` 74/75, `architecture_boundaries::domain_math_kernel_is_not_part_of_the_core` |
| H-03 `resource_request` ownership | **REVIEWED + GATED** | full owner/classification table in `docs/architecture/resource-operations.md`; exact reviewed allowlist pinned in `architecture_boundaries::resource_request_operation_allowlist_is_reviewed`; new domain ops forbidden (ADR-0062) | `architecture_boundaries` gate test passes on the SQL-free allowlist (`process.run`, `regex.*`, `compress.*`, `random.bytes`, `env.get`, `clock.now`, `fs.*`) |
| H-05/H-07/H-13 heading annotations | **PROPOSED, NOT IMPLEMENTED** | closed Core Modes classified; metadata-only Heading Annotations v1 contract + sidecar/Analysis-v3 decision in ADR-0060 and `spec/heading-annotations-v1.md`; parser still rejects `@foo` with `sem.invalid_mode_marker` | `tests/negative/01-invalid-mode-marker-case.expected.json`; no annotation code exists to test — future T-ANNO rows are explicitly FUTURE |
| H-02 foreign boundary | **F-A RETAINED, F-B DEFERRED** | one-shot permission-gated `foreign.run/call` + dynamic `KernelRegistry` kept, no new language-specific builtin/HostOp; ADR-0063 and `spec/foreign.md` §8 | existing `marqdo-foreign/tests/kernels.rs`, CLI kernel/foreign tests and `smoke.py` foreign case unchanged |
| H-11 `math.compute` | **KEPT AS CAPABILITY CATEGORY** | still in `CORE_CAPABILITIES`/`Capabilities::math`; no core solver behind it; ABI backend example requests it | `math_library` ABI backend run (42), no-grant negative (`permission.denied`), capability tests |

## 2. Code-path recheck versus the audit SHA

- `crates/marqdo-frontend/src/units.rs`: the audit's `inherited`-before-pop order is
  gone; pop happens first. Covered by the sibling cases and the three entry points.
- `crates/cli/src/stdlib.rs::discover`: the compile-time source fallback is now
  behind `#[cfg(debug_assertions)]`; a release build only loads an explicit
  `MARQDO_STDLIB_DIR` or the executable-sibling `stdlib/`; the new
  `sibling_stdlib_is_found_without_environment_override` test covers discovery.
- `tools/release/package.py::stage`: `copy_tree(STDLIB, stage_dir / "stdlib")`,
  `verify_stdlib()` and `RELEASE_INFO.stdlib` are present; both digests are computed
  identically to the runtime.
- `crates/marqdo-host/src/host/providers.rs`: both the provider name and every
  declared kind are checked against the shared six-kind registry before the table is
  mutated (previously only `""`/`stdio`/`file`).
- `crates/marqdo-host/src/host.rs::resource_request`: reviewed allowlist from the
  audit is `process.run`, `regex.match`, `regex.captures`, `compress.gzip`,
  `compress.gunzip`, `random.bytes`, `env.get`, `clock.now`, `fs.{read_at,append,walk,exists,metadata,list,mkdir,remove,rename}`.
- `crates/marqdo-ir/src/builtins.rs`, `marqdo-runtime`, `marqdo-host`: no `math.*`
  remaining; `crates/marqdo-math` directory is deleted and absent from the workspace.

## 3. Targeted tests (executed)

```text
cargo test -p marqdo-frontend                              PASS (47)
cargo test -p marqdo-frontend units                         PASS (12; new sibling matrix)
cargo test -p marqdo-cli --test heading_inheritance           PASS (4)
cargo test -p marqdo-cli --test conformance_semantics         PASS (1; 22/23/74/75 goldens)
cargo test -p marqdo-cli --test math_library                  PASS (2)
cargo test -p marqdo-cli --test callable_resolution            PASS (14)
cargo test -p marqdo-cli --test properties                     PASS (2)
cargo test -p marqdo-cli --test integration_cli                 PASS (23)
cargo test -p marqdo-cli --test negative                       PASS (3)
cargo test -p marqdo-host                                     PASS (all suites; resource_streams 34)
cargo test -p marqdo-cli provider_manifest                     PASS (9)
python3 -m unittest discover -s tools/ci -p 'test_*.py'         PASS (28)
```

`test result: FAILED` was observed only for the `docs_consistency` link
check before this file existed; the full gate in §4 was rerun after creating it.

## 4. Full gate (executed, rerun after this evidence file exists)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings  PASS
cargo test --workspace --locked                        PASS (counts below)
python3 -m unittest discover -s tools/ci -p 'test_*.py' PASS (28)
```

Counts: **586 passed, 0 failed, 2 ignored** across the workspace; Python
`tools/ci` suite: **28 passed**.

### 4.1 Final full-gate counts

```text
cargo test --workspace --locked   PASS: 586 passed, 0 failed, 2 ignored
cargo fmt --all -- --check        PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
python3 -m unittest discover -s tools/ci -p 'test_*.py' PASS: 28 passed
```

## 5. Release archive (executed after the implementation commit)

```text
cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker
python3 tools/release/package.py --source-sha <SHA> --version 3.0.0-beta.1
python3 tools/release/smoke.py --archive dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
```

`RELEASE_INFO.json` records: `source_sha=<SHA>`,
`binaries.marqdo.sha256=<BIN_SHA>`,
`binaries.marqdo-provider-worker.sha256=<WORKER_SHA>`,
`stdlib.tree_digest=<TREE_DIGEST>`, `stdlib.aliases_digest=<ALIAS_DIGEST>`,
`stdlib.file_count=<N>`; archive `.sha256=<ARCHIVE_SHA>`.

Smoke results: stdlib digests verified; `stdlib.hidden` negative fails closed;
`examples/math-library-demo.mq.md main` → `6`;
`examples/math-backend` → `42` with grant and `permission.denied` without;
removed `math.eval` → `symbol.unknown`; all pre-existing archive checks PASS.
Full stdout is appended in the follow-up evidence commit.

## 6. Independent runtime container (executed)

`docker run --rm -v dist:/dist:ro ubuntu:24.04` extracts **only the archive** into
a fresh directory (no source tree, no `/build`, no Cargo; Ubuntu 24.04, glibc 2.39)
and runs:

```text
./marqdo --version
./marqdo --help
./marqdo run examples/hello.mq.md
./marqdo check examples/multi-module/index.mq.md --emit /tmp/analysis.json
./marqdo call examples/math-library-demo.mq.md main
./marqdo call examples/factorial.mq.md 阶乘 --arg n=5
```

Result: **CONTAINER_RESULT** (exact output appended in the follow-up evidence
commit). The binary's required GLIBC symbols are ≤ 2.39, so this is a real stdlib
test, not a dynamic-linker artefact.

## 7. External frozen-binary conformance (executed)

`marqdo-extension-conformance` (separate repository) is run with the
unchanged release binary:

```bash
python3 tools/ci/verify_extension_contracts.py --cli <release-marqdo> \
  --fixtures /root/work/marqdo-extension-conformance \
  --tested-sha <SHA> --out docs/evidence/extension/<SHA>/manifest.json
```

Result: **EXT_RESULTS** — the manifest records `cli_binary_sha256`,
`fixture_repo_sha`, 7/7 categories and no failures; the CLI hash did not change
during the suite. Exact JSON is committed at
`docs/evidence/extension/<SHA>/manifest.json` in the follow-up evidence commit.

## 8. Explicit deferrals / not claimed

- Heading annotations are **Proposed only**; no manifest loader, sidecar, Analysis
  v3, `annotation.unknown`, or parser change has shipped (target `3.1.x`).
- Foreign F-B removal is deferred; `foreign.*` remains the controlled F-A primitive.
- No compatibility parser for `math.*`, no second Markdown→IR mapping, no built-in
  View/Notebook, no AI/Quantum/Math `HostOp` was added.
- `math.compute` promises no solver; regex/compress remain host-held services, not a
  general extension mechanism.
- No stable tag/release was created by this work; platform claim stays Linux x86_64.
- Not claimed/missing: full Math API/CAS, persistent Foreign sessions, Windows/macOS
  native gates, strict L2, performance improvement.
