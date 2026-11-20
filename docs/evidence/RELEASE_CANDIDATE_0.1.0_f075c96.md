# Marqdo CLI release candidate — 0.1.0 / `f075c96`

```text
Source SHA:              f075c967f24d885ac42c1e99329bee3498154598
Workspace version:       0.1.0
Intended release target: Linux x86_64 (local RC); macOS/Windows UNVERIFIED
GitHub master:           c9f01aa8fb127a2dcc125103ef794832984774ad (unchanged)
Latest public CI:        https://github.com/cflmy/marqdo-cli/actions/runs/44692692318 (FAILED)
```

This report records the convergence work and the honest release gate. The
checkout used for this run has a Gitee remote and no GitHub credentials, so no
new remote CI URL can be produced here. The code is ready for a CI run; the
release decision is **DO NOT RELEASE** until the hard gates below have remote
PASS evidence.

## 1. What changed at this SHA

- macOS/Windows `E0433`: strict detection now dispatches through `#[cfg]`
  functions, so `linux::probe()` is not type-checked on non-Linux targets.
  A non-Linux regression test asserts `strict=unavailable` and fail-closed
  refusal.
- Linux strict probe→launch race: `inspect --platform` is a snapshot, not a
  promise. The CLI test now accepts only success with correct output or a
  non-zero, diagnosed, fail-closed refusal with no project stdout. A sandbox
  seam test injects `probe OK → spawn EAGAIN` and proves `LaunchFailed`,
  never unrestricted execution.
- Parallel security matrix: unique per-test project/outside paths and a
  once-compiled helper, so `cargo test --workspace` can run the 7 adversarial
  cases in parallel.
- Condition headers: executable `Strong`/`Emph` in the description remainder
  are a hard `markdown.invalid_construct` error; condition expression spans are
  mapped from the literal bytes inside the backticks.
- Power/control-flow corpus: 10 AST, 8 semantic, 4 compile-negative and 2
  runtime-negative fixtures; CLI E2E branch/loop/inert tests.
- Generic Tool protocol: independent `sample-view`, `sample-knowledge` and
  `sample-proof` Analysis/Trace consumers, installed and dispatched by name
  with no CLI hard-coding.
- Locked path-dependency tamper test; G-C lane honours `CARGO_TARGET_DIR`.

## 2. Local static and test evidence

Ubuntu 26.04 development container, Rust 1.100.0:

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
cargo build --release --locked -p marqdo-cli          PASS
```

G-C VM, source archive `f075c96`, ordinary user `dev` (uid 1000):

```text
MARQDO_REQUIRE_STRICT_SECURITY=1 ./tests/security/run-lane.sh
  adversarial 7/7 PASS
  positive CLI contract PASS
  external surfaces refused PASS
  forged marker rejected PASS
  G-C security lane: PASS
cargo test --workspace --locked (same archive)       PASS (47 binaries, 0 failed)
```

See [`2026-11-20-strict-gc-f075c96.md`](2026-11-20-strict-gc-f075c96.md).

## 3. Release candidate artifact

```text
archive:  marqdo-0.1.0-x86_64-unknown-linux-gnu.tar.gz
sha256:   4e1020735bd7f5bd590b5472e48ed431b1365641b8ac6574254dd963be8cc285
contents: marqdo binary, MIT/Apache licenses, CHANGELOG, COMPATIBILITY,
          README, spec/, examples/, tests/consumers/, selected docs,
          SOURCE_SHA, rustc -vV
```

Clean-install smoke from an extracted external directory:

```text
sha256sum -c marqdo-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256 -> OK
./marqdo --version                     -> marqdo 0.1.0
./marqdo --help                        -> ok
./marqdo run examples/hello.mq.md      -> 你好，Marqdo
./marqdo check examples/multi-module/index.mq.md --refs --emit ... -> ok
analysis_consumer.py                   -> functions=2
trace_consumer.py                      -> events=7
validate_protocols.py                  -> validated analysis/trace/tool
```

No Windows or macOS archive was produced from this host. The `release-candidate`
GitHub job can build macOS, but it has not run for this SHA.

## 4. Gate matrix

| Gate | Status at f075c96 | Evidence / next action |
|---|---|---|
| G0 same SHA, version, changelog | **PASS (local)** | source archive `f075c96`; version 0.1.0; changelog updated; remote master still `c9f01aa` |
| G1 Linux CI fmt/clippy/workspace/smoke | **FAIL — no remote same-SHA run** | local PASS; push to GitHub and let `CI` run |
| G2 Windows workspace + portable smoke | **FAIL — no remote native run** | cfg fix is local; needs Windows CI |
| G3 macOS workspace + portable smoke | **FAIL — no remote native run** | cfg fix is local; needs macOS CI |
| G4 public Schema + consumers | **UNVERIFIED at same SHA** | local schema/consumer PASS; remote job last passed at `c9f01aa` |
| G5 Markdown AST/power new semantics | **PASS (local)** | 160 AST + 44 semantic fixtures; CLI E2E |
| G6 negative/source-span/inert tests | **PASS (local)** | 52 compile-negative, 12 runtime-negative, span unit/CLI tests |
| G7 lock v3 + package environments | **PASS (local Linux)** | package_env, Git/path tamper, locked sync; Windows/macOS UNVERIFIED |
| G8 Tool/ABI/Worker/Foreign matrix | **PASS (local Linux, bounded)** | generic Tool + 3 consumers; native/worker/foreign tests; Windows ABI UNSUPPORTED |
| G9 Linux strict L1 G-C | **PASS (local VM); CI UNVERIFIED** | 7/7 + CLI; needs self-hosted security-lane URL |
| G10 RC build/checksum/clean install | **PASS (Linux only)** | artifact + sha256 + external clean-install smoke |
| G11 docs/version/license/compatibility | **PASS** | STATUS, COMPATIBILITY, CHANGELOG, security evidence |
| G12 owner approval + tag/Release | **NOT STARTED** | requires explicit project-owner approval |

## 5. Decision

**DO NOT RELEASE.**

Reason: G1/G2/G3 have no real GitHub PASS at this SHA, G4 is not re-proven at
this SHA, and G9 has no self-hosted security-lane URL. Local VM and container
evidence cannot be substituted for those lanes.

## 6. Next actions (in order)

1. Push `f075c96` or its docs-complete successor to GitHub `master` and wait
   for all `CI` jobs (Linux, macOS, Windows, schema/consumers) to pass.
2. Register the disposable security VM as a self-hosted runner with labels
   `[self-hosted, linux, marqdo-security]`, then run
   `.github/workflows/security-linux.yml` at the same SHA. Record the URL and
   7/7 result.
3. Run the `release candidate artifact` job at the same SHA; download and
   verify the Linux/macOS archives and checksums in a clean directory.
4. Only after G1–G11 have same-SHA URLs, ask the project owner for explicit
   approval, then create the version tag and publish the release.

## 7. Known limitations / rollback

- Linux strict L1 only; strict L2 (Worker/Foreign/native) is refused.
- Windows Native/Worker ABI is unsupported.
- macOS/Windows strict is fail-closed, not implemented.
- Foreign requires a pre-provisioned, digest-pinned environment; no uv/npm/venv
  auto-provisioning.
- The RC has no signed provenance/SLSA attestation yet; checksums are the only
  integrity artifact.
- Rollback is to keep the previous stable release (none has been published) and
  withdraw the candidate tag/artifact; no data migration is involved.
