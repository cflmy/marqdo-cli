# RFC-EXT-0001 implementation evidence (P0 core + PR-03/05/06/07 first pass)

Status: **honest record of the implementation tree.** PR-00/01/02/03/04/05/06
are implemented and tested on Linux x86_64; PR-07/PR-08 additional work is the
SHA-bound evidence manifest and the tagged-release rerun. This document is not a
release claim. The authoritative executed status is regenerated in
`docs/STATUS.md` and `docs/evidence/extension/<SHA>/manifest.json`.

## 1. Identity and environment

| Field | Value |
|---|---|
| Trees | `/root/work/marqdo-cli` (branch `feat/extensible-ecosystem`) and separate `/root/work/marqdo-extension-conformance` |
| Baseline | `595279fe38048b52b96d9ff1e7d57a867ec5c4c6`; green baseline in `docs/evidence/ext-baseline-595279f.md` |
| Rust | `rustc 1.100.0`, `cargo 1.100.0` |
| OS / arch | Linux x86_64 |
| Commands | logged under `.cache/ext/` (gitignored) |

No commit SHA is claimed for the implementation yet: work is uncommitted on the
feature branch, so this file cannot be a same-SHA release artifact.

## 2. Executed workspace gate (current tree)

```text
cargo fmt --all -- --check                            PASS  (exit 0)
cargo clippy --workspace --all-targets -- -D warnings PASS  (exit 0)
cargo test --workspace --locked                       PASS  (exit 0)
  67 test targets: 599 passed, 0 failed, 0 ignored
```

Targeted new suites:

```text
cargo test -p marqdo-cli --test callable_resolution      14 passed
cargo test -p marqdo-cli --test stdlib_alias_routing     5 passed
cargo test -p marqdo-cli --test provider_cli             12 passed
cargo test -p marqdo-cli --test analysis_trace           7 passed
cargo test -p marqdo-cli --test extension_selection      13 passed
cargo test -p marqdo-cli --test plugin_worker            4 passed
cargo test -p marqdo-cli --test print_routing            2 passed
cargo test -p marqdo-cli --test architecture_boundaries  9 passed
cargo test -p marqdo-host                                (includes capability_handlers 12)
```

## 3. What was implemented (source paths)

### PR-01/02 — one static binding, bound runtime dispatch

* `crates/marqdo-ir/src/call_binding.rs` — `CallSiteKey`, `BoundTarget`
  (`Builtin`/`Marqdo`/`AbiPlugin`/`DynamicValue`), deterministic
  `CallBindingTable`, unit tests.
* `crates/marqdo-ir/src/callable.rs` — `try_from_program` (rejects duplicate
  exports instead of first-wins), `OfficialAlias` registration, dotted-prefix
  conflict helpers, `function_id`.
* `crates/marqdo-frontend/src/calls.rs` — the single `bind_program_calls` pass,
  RFC priority matrix, import namespace conflicts, exact plugin signature
  validation; `validate_module` is a standalone-frontend wrapper.
* `crates/marqdo-frontend/src/analyze.rs` — `analyze_source_for_program` defers
  call checking to the composition root; standalone `analyze_source` keeps its
  single-document checks for the conformance corpora.
* `crates/marqdo-runtime/src/interpreter.rs` — dispatch by `CallSiteKey`; the
  unknown-plugin fallback, `print_override` branch and dead `resolve_function`
  are removed; bound ABI calls normalise positional arguments to manifest order.
* `crates/cli/src/extension_descriptors.rs` — static plugin manifest reading
  (no `dlopen`/spawn), signature parsing (defaults rejected), bundle-relative
  path checks, artifact sha helpers.
* `crates/cli/src/project_prepare.rs` — one `prepare()`: load → descriptors →
  registry → bind once → `PreparedProgram`; diagnostics are appended once.
* `crates/cli/src/commands/{check,run,inspect}.rs` + `loader.rs` — all consume
  `prepare_project`; `run` verifies artifacts before load and reconciles the
  Host exports one-to-one (`ext.plugin_binding_mismatch` on any difference).
* `crates/cli/src/analysis.rs` — Analysis `calls[].state/target` are projected
  from the same binding table (no second resolver).
* `crates/cli/tests/callable_resolution.rs` — CALL-01/02/03/04/05/06/07/08/09/
  11/12/13/14/15/16 as black-box CLI tests (CALL-10 has unit coverage in
  `commands/run.rs` for missing/extra runtime exports).

Normative docs: `spec/callable-resolution.md`, `docs/adr/0054-*`,
`spec/plugin-abi.md` §12, `spec/analysis-v2.md` calls note.

### PR-03 — resource provider v2 through the normal CLI

* `crates/cli/src/main.rs` — repeatable `--provider PATH` and
  `--allow-provider NAME` on `run`/`call` only.
* `crates/cli/src/commands/sandbox.rs` — grants map to
  `allow_resource_provider`; `pure`/`strict` preflight refuse the flags;
  wildcards rejected.
* `crates/cli/src/provider_manifest.rs` — strict schema v2 parser/verifier,
  target selection, bundle containment, sha256, reserved-kind rejection,
  duplicate kind/provider checks, sibling `marqdo-provider-worker` resolution
  (no `PATH` fallback; `ext.worker_unavailable`/`ext.platform_unsupported`).
* `crates/cli/src/commands/run.rs` — verify all manifests/artifacts/grants and
  duplicate kinds before spawning; spawn only granted+verified providers; no
  native fallback; no new `HostOp`/`resource_request` branch.
* Tests: `crates/cli/tests/provider_cli.rs` (real C sensor + real worker
  helper), 12 tests; `spec/resource-provider-v2.md`, ADR-0055, README section.

Provider evidence recorded by the PR-03 implementer:

| ID | Result | Evidence summary |
|---|---|---|
| PROV-01 | PASS | exit 0, `count=5`, `eof=true`, one worker/open/close marker |
| PROV-02 | PASS | exit 1 `permission.denied`; helper never started |
| PROV-03 | PASS | exit 2 `ext.provider_grant_unmatched`; no worker |
| PROV-04 | PASS | exit 2 `ext.duplicate_provider_kind`; no worker |
| PROV-05 | PASS | one provider, two kinds, counts `[5,2]`, both closed |
| PROV-06 | PASS | `stdio/file/tcp/tls/udp/tcp-listener` rejected `ext.manifest_invalid` |
| PROV-07 | PASS | sha mismatch / symlink escape / `..` → `ext.artifact_integrity` before spawn |
| PROV-08 | PASS | `ext.no_matching_artifact` |
| PROV-09 | PASS | v1 → `ext.unsupported_provider_schema`, no migration |
| PROV-10 | PASS | pure `cli.invalid_grants`, strict `strict.unsupported_grant`, `*` rejected |
| PROV-13 | PASS | sibling helper missing → `ext.worker_unavailable`, PATH helper unused |

Platform: provider worker transport is claimed **linux/x86_64 only**; macOS and
Windows are explicitly `UNSUPPORTED`/fail-closed (`ext.platform_unsupported`),
not claimed supported. PROV-11/12/14/15 belong to host-protocol and PR-07
external closure and are not claimed by the PR-03 CLI matrix.

### PR-05 — capability handler registry

`crates/marqdo-host/src/capability.rs` now has a trusted handler directory,
`fs.read` as a real handler, decide → handler-installed → execute under budget →
audit with duration; missing handler stays `capability.unavailable`, duplicate/
reserved/empty registrations are rejected. `crates/marqdo-host/tests/
capability_handlers.rs` has 12 CAP tests (CAP-01…CAP-12); ADR-0057 and
`spec/security.md` updated. `cargo test -p marqdo-host` PASS.

### PR-06 — official stdlib public aliases

`stdlib/stdlib.json` declares `public_aliases` (`print`/`打印` →
`io.mq.md::写值`) and `aliases_digest`; `crates/cli/src/stdlib.rs` verifies the
metadata digest and expected target signature; `loader.rs` loads the target
module only when the alias name is referenced and produces `OfficialAlias`
records; `project_prepare` registers them and the binding pass targets the
ordinary `Marqdo` function. `program_uses_print`, `Program.print_override`, the
runtime print branch and the AST scanner are deleted; the placeholder builtin
still fails with `kernel.unavailable` for unbundled embedders. Tests:
`crates/cli/tests/stdlib_alias_routing.rs` (5) + existing print/format/Trace
goldens; the stdlib fixture aliases that collided with builtin namespaces
(`table`/`text`/`fs`) were renamed and the bundle digest regenerated.
`tools/pack-stdlib.py` verifies `aliases_digest` before packaging.
ADR-0058 documents the decision.

### PR-07 (first pass) — independent external repository

`/root/work/marqdo-extension-conformance` is a separate Git repository (own
history, remote `https://git.cflmy.cn/cflmy/marqdo-extension-conformance`,
pushed; commit `b729afc` at the time of this record). It contains:

* `pure-lib/` + `consumer/` with a pinned full-SHA Git dependency;
* `plugin/` — real C ABI v1 worker plugin, three exports under two prefixes;
* `kernel/` — `mocklang` provider with a language the CLI does not know;
* `provider/` — `provider.json` v2 + real C ABI sensor library;
* `tool/` — stdlib-only Python consumer of Analysis v2 / Trace v2;
* `tests/run_all.py`, `tests/verify_frozen_cli.py`, `CONTRACT.md`,
  `.github/workflows/conformance.yml`.

Provisional run against the current development binary (debug build, not a
release candidate):

```text
[PASS] plugin_worker, callable_registry, foreign_kernel,
       resource_provider_cli, package_pinned_git,
       tool_analysis_consumer, negative_security
       (all categories PASS; frozen CLI sha256 recorded in evidence/results.json)
```

This is **not** final PR-07/PR-08 evidence: the binary was not a `--release`
candidate, and `tools/ci/verify_extension_contracts.py` has not yet emitted a
`docs/evidence/extension/<SHA>/manifest.json`. The external suite will be rerun
against the frozen release binary after PR-04 lands.

## 3a. PR-04 — extension selection and lock v4

* `crates/marqdo-package/src/lib.rs`: `ExtensionKind`/`ExtensionSource`/
  `Extensions` with only `./path` and `@alias/path` forms; lock v4
  `LockedExtension` identity records, deterministic ordering, duplicate/
  idempotent merge, `validate_extension_selection`/`validate_extension_declarations`,
  explicit v3 migration error.
* `PackageResolver::resolve_extension_sources`: resolves to canonical regular
  files inside the owning package (project or dependency), no execution.
* `crates/cli/src/project_prepare.rs`: merges project-declared plugin sources
  with `--plugin`, compares declared identity with the lock, fails closed with
  `ext.extension_lock_mismatch`; `pure`/`strict` refuse activation in `run`.
* `crates/cli/src/extension_selection.rs`: static identity computation for
  plugin/kernel/provider manifests (declared digests; no binary execution).
* `crates/cli/src/commands/package.rs`: `lock` writes v4 extension identities;
  `sync` validates declarations/identity before materialization.
* `crates/cli/src/commands/inspect.rs`: new `inspect --extensions` reports
  declared sources, lock status, static plugin descriptors and stdlib aliases;
  authorization is always `not_granted`.
* Tests: `crates/cli/tests/extension_selection.rs` (13), `crates/marqdo-package`
  unit tests; docs `spec/extension-selection.md`, `spec/package-format.md` v4,
  ADR-0056.

## 4. Known gaps / deviations (not hidden)

1. **Frozen release evidence is not yet bound to a tagged SHA.** The external
   suite PASSes against a `--release` build of this tree, but
   `docs/evidence/extension/<SHA>/manifest.json` must be regenerated on the
   final release commit and every mandatory result must be PASS there.
2. **Provider platform claims are Linux x86_64 only**; macOS/Windows are
   explicitly unsupported for the worker transport. The RFC's mac/Windows
   provider lanes are not claimed.
3. **Additive provider error codes** `ext.manifest_invalid`,
   `ext.duplicate_provider`, `ext.provider_grant_unmatched` were added because
   the RFC's frozen code list has no precise code for malformed v2, duplicate
   provider names, or an unmatched `--allow-provider`; documented in
   `spec/resource-provider-v2.md` and ADR-0055.
4. **`--allow-provider` without a selected manifest** aborts with
   `permission.denied` before spawning rather than silently ignoring the grant
   (needed to satisfy PROV-02 with the existing spawn-on-register Host API).
5. **Analysis v2 `calls[].state/target` semantics changed for official aliases**
   (print now projects as a `function` target). This is a projection of
   existing fields, documented in `spec/analysis-v2.md`/ADR-0054/ADR-0058;
   there is no v3 bump and the private binding table is not exposed.
6. **CALL-10's host-side mismatches** are unit-tested for missing/extra exports;
   a live TOCTOU signature/digest mismatch is covered by artifact-tamper tests
   plus exact reconciliation, not by swapping a manifest mid-run.
7. Remaining PR-08 step: regenerate the SHA-bound external evidence on the
   tagged release commit (the generator refuses a dirty checkout), then run the
   existing G3/G4/native lanes; local/container smoke is not a release claim.
