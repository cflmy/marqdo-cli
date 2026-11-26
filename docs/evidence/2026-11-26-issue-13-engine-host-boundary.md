# Evidence — issue #13 Engine / Host Contract boundary (candidate)

Date: 2026-11-26 (Asia/Shanghai)
Issue: <https://git.cflmy.cn/cflmy/marqdo-cli/issues/13>
Decision: [ADR-0071](../adr/0071-engine-host-contract-boundary.md)
Branch: `architecture/issue-13-engine-host-boundary`
Baseline: `master` at `4bbefc92449ee480c8a2c89b0308feb4cda8d0e7`
Candidate: the merge candidate built from this branch; the PR records the exact
source SHA. This file is not a release approval and does not reuse RC.4 evidence.

## Scope actually landed (M0–M3 core)

| Item | State |
|---|---|
| Neutral `marqdo-host-contract` crate (`Host`, `HostOp`, `ExecutionContext`, `ResourceRead`, `FrameId`, `ParameterSource`, `LoopTransfer`, `RuntimeObservation`) | **implemented** |
| `marqdo-runtime` depends on the contract, not `marqdo-host` | **implemented** (runtime test `marqdo-host` remains a dev-dependency only) |
| `marqdo-host` implements the contract; no reverse dependency on runtime/frontend/IR | **implemented** |
| Independent real consumer `tests/embedding` (own workspace + lock), Markdown→frontend→IR→runtime, bounded test host | **implemented and executed** |
| `marqdo-plugin-abi` reduced to the public contract (C ABI, wire, errors) | **implemented** |
| Loader/worker/manifest/artifact moved to `marqdo-host::plugin` | **implemented** |
| SDK consumes only the contract (no libloading/worker/host) | **implemented** |
| Real SDK plugin built and invoked by the real CLI; tampered digest rejected | **implemented and executed** |
| Dependency-direction gate from Cargo metadata + negative unit cases | **implemented** |
| Current architecture/doc entry, ADR, historical banners, status | **implemented** |
| New remote repository (`marqdo-cli-*`, engine/host) | **deliberately not created** — issue #13 §9.2 admission is not met; keep-in-workspace is recorded in ADR-0071/CURRENT.md §7 |
| Stdlib Bundle Phase B, SDK physical migration, regex/compression migration, foreign F-B | **not in this candidate**; separate issues/decisions, listed in `docs/STATUS.md` |

## Commands and results

All commands were run in this checkout with the repository-local Rust toolchain
(`rustc 1.100.0`, `cargo 1.100.0`) and `--offline` where the environment requires
the warm cache. The candidate source SHA is on the PR; do not copy these results
to a different commit.

| Gate | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check` | PASS |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| Rust tests | `cargo test --workspace --locked --offline --no-fail-fast` | PASS — **636 passed / 0 failed / 2 ignored** across 74 test binaries (includes `docs_consistency`, `architecture_boundaries`, `stdlib_product`); baseline `4bbefc9` in a detached worktree is **633 passed / 0 failed / 2 ignored** (no regression; the candidate adds three architecture-boundary tests) |
| Architecture boundaries | `cargo test -p marqdo-cli --test architecture_boundaries --locked` | PASS (15) |
| Docs consistency | `cargo test -p marqdo-cli --test docs_consistency --locked` | PASS |
| Dependency gate | `python3 tools/ci/check_dependency_boundaries.py` | PASS |
| Dependency gate negative cases | `python3 -m unittest tools.ci.test_dependency_boundaries` / `python3 -m unittest discover -s tools/ci` | PASS (54 tool tests) |
| Engine independence and differential execution | `python3 tools/ci/check_engine_boundary.py` | PASS |
| Plugin SDK real consumption | `python3 tools/ci/check_plugin_sdk.py` | PASS at that candidate; the SDK and this gate moved to the independent repository under issue #15 / ADR-0072 (evidence: `2026-11-26-issue-15-sdk-independent-repository.md`) |
| Existing product/conformance suites | included in workspace run; stdlib product suite additionally run | PASS |

Behavioral facts from the engine gate:

- the independent consumer returns `sum_to(10) = Int(55)` and a structural
  `types(7)` map whose terminal Value Wire bytes equal a real CLI `call
  --output json` on the same file;
- the consumer rejects real fuel exhaustion, stack-depth exceedance, expired
  deadline, out-of-bounds `Memory` access, a host that grants no external effect
  (`permission.denied`), and a host with no resource implementation
  (`resource.unsupported`);
- `cargo tree -p marqdo-runtime -e normal,build` in the consumer graph has no
  `marqdo-host`, sandbox, foreign or libloading/rustls dependency;
- `marqdo-plugin-sdk` normal tree contains only `marqdo-plugin-abi`,
  `marqdo-value` and serialization primitives.

Behavioral facts from the SDK gate:

- `tests/sdk-plugin` builds with only `marqdo-plugin-sdk` as a path dependency;
- the real CLI loads it with `--trust-native-plugin` and
  `--allow-plugin sdk.demo` and returns `Int 42`;
- a tampered `sha256` in `plugin.yaml` is rejected with an artifact/sha256
  diagnostic.

## CI evidence (remote, same code SHA)

Woodpecker PR pipeline **#101** for commit
`9b4f0a08228e9932bf3b0a8c5cf80e0d04626ccb` finished **success** with every step
green: `clone`, `contract`, `test`, `minimum-rust`, `build`. The `build` step
builds the release binaries, packages the deterministic archive for
`$CI_COMMIT_SHA`, runs the from-archive smoke test and the frozen external
conformance suite, so C11/G05 get same-candidate CI evidence rather than an old
release claim. The first pipeline (#100) failed in one unrelated pre-existing
`provider_cli` test with `ETXTBSY` while executing a freshly copied binary; the
same tree passed on straight rerun (#101), so it is recorded as an environment
flake, not a candidate defect.

This evidence file was updated by a docs-only commit after that code SHA; code
and lock bytes are unchanged by it. The PR check list shows the final run.

## Not verified here (honest state)

| Item | State | Reason / owner |
|---|---|---|
| Minimum Rust 1.89 | **UNVERIFIED** locally | only Rust 1.100 is installed in this environment; the Woodpecker `minimum-rust` lane runs `cargo check --locked --workspace --all-targets` on `rust:1.89-bookworm` |
| Windows GNU cross-target compile | **PASS (cross-check only)** | `cargo check --workspace --all-targets --locked --target x86_64-pc-windows-gnu` finished successfully (pre-existing unrelated warnings); native Windows runtime and the GitHub/Woodpecker native lanes remain the actual platform gate |
| Windows/macOS native runtime | **UNVERIFIED** locally | no native runner here; existing GitHub/Woodpecker lanes remain the gate |
| `wasm32-unknown-unknown` Engine probe (E12) | **UNVERIFIED** | target std not installed and no network here; `cargo check -p marqdo-runtime --target wasm32-unknown-unknown` fails with `E0463: can't find crate for core/std`, i.e. a missing target toolchain rather than a forbidden native dependency. E01 is green, so no P0 violation is implied, but this is not a PASS |
| strict OS-isolation lane | **UNVERIFIED** | needs the disposable VM; `strict` remains fail-closed |
| Performance/RSS budget (G06) | **UNVERIFIED** | shared-node measurement is not valid evidence; no performance claim is made |
| Frozen external conformance / release archive (C11/G05) | **CI PASS at code SHA `9b4f0a0`** | Woodpecker pipeline #101 `build` step: release archive from that SHA, archive smoke, frozen external conformance, PASS. This is not a new release tag. |
| SDK physical repository, stdlib Phase B, regex/compression migration | **DEFERRED** | separate issues; see `docs/STATUS.md` |

## Boundary confirmation

- No new top-level CLI command, no `HostOp`/builtin/Value variant, no second
  interpreter, no `legacy` feature, no compatibility re-export of removed
  `marqdo_host::{Host, HostOp, ...}` paths.
- Public protocols and their revisions are unchanged. This evidence covers an
  internal Rust boundary change, not a protocol migration.
- No remote repository was created or modified for this work beyond pushing the
  candidate branch and PR; no empty candidate repository exists.
