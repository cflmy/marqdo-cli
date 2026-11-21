# Beta release gates — `3.0.0-beta.1`

Date: 2026-11-22. Product: `3.0.0-beta.1`. Release SHA:
`2c7c4f31e7a388d3fcfa717a77067e696ed13854` (tag `v3.0.0-beta.1`). Platform: Linux x86_64 (glibc).
Profile: **beta** (`tools/ci/release_evidence.py --profile beta`).

This document records the exact beta gate evidence for the tagged SHA. The
machine-readable aggregate is
[`ci/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`](ci/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json)
(decision `BETA_RELEASE_CANDIDATE`). The stable profile is stricter and is not
applied here.

## Release artifacts

```text
source SHA          2c7c4f31e7a388d3fcfa717a77067e696ed13854
archive             marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
archive sha256      507e4cd358f75f517433935ce90d5fc2eb5cecda4f19121a68b136c83b15f317
archive size        4701997 bytes
marqdo sha256       062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f
worker sha256       7266db75f52e2954b29f8f018d957e1cae3110a239f98dfe1f34b73c90c96ba2
target              x86_64-unknown-linux-gnu
toolchain           rustc/cargo 1.100.0 (Cargo.lock committed, --locked)
```

The archive contains `marqdo` and `marqdo-provider-worker` in the same
directory, `RELEASE_INFO.json`, `SOURCE_SHA`, `KNOWN_LIMITATIONS.md`,
licenses, README/CHANGELOG/COMPATIBILITY, `spec/`, `docs/developers/`,
`examples/` (with host-built plugin/provider artifacts) and
`tests/consumers/`.

## Gate results

| Gate | Result | Evidence |
|---|---|---|
| Source hygiene | PASS | clean tree at tagged SHA; `Cargo.lock` committed; `--locked` build |
| fmt / clippy / tests | PASS | same-SHA Woodpecker pipeline #37 (test step); local gate log |
| Release build (CLI + worker) | PASS | both binaries in archive, same target; `RELEASE_INFO.json` |
| Same-SHA Linux CI | PASS | https://ci.cflmy.cn/repos/1/pipeline/37 (commit `2c7c4f31e7a388d3fcfa717a77067e696ed13854`) |
| Frozen-binary external conformance | PASS | [`extension/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`](extension/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json), 7/7 categories, CLI hash unchanged, fixture repo `e22ec95b097477ae9c19d3bfe19048af5337b806` |
| Clean-install smoke | PASS | `tools/release/smoke.py` from the archive only; see below |
| Version compatibility | PASS | prerelease Tool/project SemVer tests + `project.marqdo` fail-closed tests in pipeline #37 and local `cargo test` |
| Strict policy (G3) | PASS | same-SHA disposable-VM lane: [`2026-11-22-strict-gc-2c7c4f31.md`](2026-11-22-strict-gc-2c7c4f31.md), `outcome: PASS`, uid 1000, `container: none` |
| Performance (G4) | WARN (Beta-approved) | 9/9 correctness-verified on fixed Woodpecker agent 1, shared powersave host: [`2026-11-22-perf-woodpecker-agent-1-run38.md`](2026-11-22-perf-woodpecker-agent-1-run38.md) |
| Support matrix | PASS | Linux x86_64 supported; macOS/Windows explicitly experimental/unsupported: [`../../COMPATIBILITY.md`](../../COMPATIBILITY.md) |
| Third-party guide validation | PASS | [`2026-11-22-beta-guide-validation.md`](2026-11-22-beta-guide-validation.md) and its log |

## Clean-install smoke (archive only)

Executed outside the source tree on the extracted archive:

```text
smoke: archive version 3.0.0-beta.1, source 2c7c4f31e7a388d3fcfa717a77067e696ed13854
smoke: strict available (bubblewrap 0.11.1 ...) and exercised
smoke: PASS
```

Covered: `.sha256` verification; `RELEASE_INFO.json`/`SOURCE_SHA` and binary
hashes; offline `--version`/`--help`; check/run/call; Analysis v2 + Trace v2
consumers (with schema validation when `jsonschema` is installed); Tool
install/dispatch; native and worker plugin transports; foreign kernel; Provider
v2 through the shipped worker (positive read, unauthorized denial, missing
helper `ext.worker_unavailable`); pure mode; strict when the host supports it
(otherwise the tested fail-closed refusal). The CI pipeline also builds and
smoke-tests its own archive in the build step, so the packaging procedure is
reproduced by CI, while the **published** archive is the locally built one whose
hashes are bound above.

## Explicit gaps (allowed for beta)

- macOS/Windows release artifacts, native plugin/provider ABI and provider
  worker: not claimed and not built; the Windows lane is a compile gate only.
- Strict L2, Wasm, uv/npm auto-environments, package registry, JIT, built-in
  Knowledge/Proof/View: unsupported/deferred (see `KNOWN_LIMITATIONS.md`).
- G4 is a Beta-only WARN on a shared powersave agent, not a stable performance
  guarantee.
- No LTS or stable `3.0.0` claim; beta breakage is allowed with changelog notes.

## Approval

The owner directed execution of the `3.0.0-beta.1` prerelease per
[`MARQDO_3_0_PRERELEASE_PLAN.md`](../MARQDO_3_0_PRERELEASE_PLAN.md) in this
session (2026-11-22). The tag is created only from the SHA above after every
gate in this table is recorded; the G4 Beta-only WARN and the Linux-only beta
support scope are explicitly approved as prerelease exceptions, not stable
exceptions. Rollback and withdrawal rules are in
[`releasing.md`](../releasing.md) §7.
