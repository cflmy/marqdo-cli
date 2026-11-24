# CI/CD

## Current setup

- Git remote: `https://git.cflmy.cn/cflmy/marqdo-cli.git`
- Woodpecker: `https://ci.cflmy.cn`
- Pipeline definition: [`.woodpecker/main.yml`](../.woodpecker/main.yml)
- Current hardening plan/RFC:
  [`MARQDO_CI_SECURITY_PLATFORM_PERFORMANCE_RFC_20261121.md`](MARQDO_CI_SECURITY_PLATFORM_PERFORMANCE_RFC_20261121.md)
- Credentials are local-only and ignored; see
  [development-credentials.md](development-credentials.md). Do not put tokens in
  pipeline files or commit messages.

The default Docker pipeline runs on push and pull request. A fast `contract`
lane exercises the public Analysis/Observation contracts through the real CLI
and the independent Python consumers; the full `test` lane runs the Rust gates
and the Windows cross-target compile check; `build` packages the archive, runs
the from-archive smoke and the frozen external conformance suite.

```text
contract (rust:1.100-bookworm, python3-jsonschema, both shipped binaries):
  cargo build --locked -p marqdo-cli -p marqdo-provider-worker
  real CLI check/run over the Observation fixtures
  analysis_consumer / observation_consumer / validate_protocols
  stream==snapshot assertion (tools/ci/verify_observation_stream.py)
  stale-Analysis rejection

test (rust:1.100-bookworm, clippy/rustfmt, Node.js 22, MinGW, both binaries):
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo build --locked -p marqdo-provider-worker
  cargo test --workspace --locked
  docs_consistency / architecture_boundaries / stdlib_product
  python3 -m unittest discover -s tools/ci -p "test_*.py"
  rustup target add x86_64-pc-windows-gnu
  cargo check --workspace --all-targets --target x86_64-pc-windows-gnu

minimum-rust (rust:1.89-bookworm):
  cargo check --locked --workspace --all-targets

build (depends on test, minimum-rust, contract):
  cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker
  package the deterministic archive for $CI_COMMIT_SHA
  from-archive smoke (tools/release/smoke.py)
  frozen external conformance (tools/ci/run_frozen_extensions.sh, 7/7)
```

A separate manual workflow, [`.woodpecker/performance.yml`](../.woodpecker/performance.yml),
builds the beta baseline and the candidate archive on the shared Woodpecker
agent and records the nine G4 comparisons. Its reports stay `SMOKE`; only the
maintainer-approved RC `WARN` exception can use them (see
[`releasing.md`](releasing.md) and [`performance.md`](performance.md)).

The pipeline is a protocol consumer itself: it must exercise only the current
Analysis and Observation contracts. `docs_consistency::ci_configuration_uses_only_current_protocols`
fails if a retired option or consumer name returns, and
`current_contract_documents_do_not_reference_retired_interfaces` extends the
same rule to `spec/` and the current developer guides.

Node.js 22 is required because the optional foreign-kernel tests use ES-module
syntax; Debian bookworm's Node 18 fails those tests.

> **Language contract note.** `cargo test --workspace` includes conformance
> fixtures that execute `.mq.md` source under
> [RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md). A red
> executable-source suite is a real regression, not an expected migration
> state. Do not "fix" it by restoring the old mapper or regenerating goldens;
> see [`STATUS.md`](STATUS.md). The lanes above are the current definition; it
> changes only with this document.

## What this pipeline does not cover

- **Strict security lane**: needs a disposable VM/self-hosted runner with
  Bubblewrap/AppArmor namespace support. See [security-vm.md](security-vm.md)
  and [security-gate.md](security-gate.md). The Docker pipeline is not an
  OS-isolation proof.
- **Windows/macOS native lanes**: the GitHub-hosted
  `.github/workflows/ci.yml` matrix **does exist and does run** on
  `windows-latest` and `macos-latest`. The Woodpecker instance itself has no
  Windows/macOS agent, so a Woodpecker-only green pipeline cannot substitute
  for those native jobs. A remote Windows/macOS PASS requires the GitHub
  matrix (or an equivalent real native runner) on the same SHA.
- **Performance baseline (G4)**: needs a fixed Linux node; see
  [performance.md](performance.md). Development-container or TCG/QEMU timings
  are `SMOKE`, never release evidence.
- **Release publication/signing**: no publication job is defined. The
  `release-evidence` job produces a SHA-bound manifest artifact; it does not
  tag or upload a release.

## RFC-EXT-0001 CI/CD additions

* The Woodpecker **test** lane builds `marqdo-provider-worker` before the
  workspace tests so the provider CLI tests use the real helper, and adds a
  `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`
  compile gate (native Windows/macOS runtime lanes stay on the GitHub matrix).
* The Woodpecker **build** lane and the GitHub **release-candidate** lane build
  `marqdo` **and** `marqdo-provider-worker` and package both as siblings; the
  provider transport resolves only that sibling (no PATH fallback), so an
  archive without it would silently lose provider support.
* `tools/ci/test_verify_extension_contracts.py` covers the external-extension
  evidence validator in the existing `python3 -m unittest discover -s tools/ci`
  step.
* The release-evidence job validates
  `docs/evidence/extension/<SHA>/manifest.json` when it is committed for the
  target SHA and prints `UNVERIFIED` otherwise; generating it is the mandatory
  release action described in `docs/releasing.md` §7, not a substitute for
  G3/G4/native lanes.

## Local reproduction

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli
```

## Local API/trigger

The ignored `.secrets/dev.env` may contain a Woodpecker API token for local
queries/triggers. The token is not needed by the pipeline itself and must never
be committed.

## External extension evidence (RFC-EXT-0001 PR-07/PR-08)

A release candidate must also prove that a **frozen** CLI binary can use
extensions from physically separate repositories without modifying or
rebuilding this workspace. This is a new necessary condition; it does **not**
replace the security/platform/performance gates above.

Required inputs:

* a release build of this commit: `cargo build --release --locked -p marqdo-cli`;
* the provider helper shipped next to it for the same target:
  `marqdo-provider-worker` (the provider transport resolves only that sibling;
  no `PATH` fallback);
* a clean checkout of the external fixture repository
  `https://git.cflmy.cn/cflmy/marqdo-extension-conformance`.

Generation and validation:

```bash
CLI="$(realpath target/release/marqdo)"
python3 tools/ci/verify_extension_contracts.py \
  --cli "$CLI" \
  --fixtures /path/to/marqdo-extension-conformance
python3 tools/ci/verify_extension_contracts.py \
  --validate docs/evidence/extension/<40-hex-source-SHA>/manifest.json
```

The generator hashes the CLI before and after the external suite, records the
source SHA and fixture repo SHA, and refuses to write evidence for a changed
binary, a dirty fixture checkout, a `FAIL`, or an unexplained
`UNSUPPORTED`/`SKIPPED` result. The external suite itself
(`tests/run_all.py` in that repository) uses only `$MARQDO_BIN` (absolute),
never Cargo, and covers: pinned-Git `.mq.md` package, worker ABI plugin,
unknown foreign kernel, provider v2 worker, and an independent Analysis/Trace
consumer.

The schema is [`evidence/extension/schema.json`](evidence/extension/schema.json);
the generator is `tools/ci/verify_extension_contracts.py`. A manifest whose
`tested_sha` is not the release SHA is invalid. Platform-specific results must
be reported as `UNSUPPORTED` (for example provider worker on non-linux/x86_64);
a Linux PASS never substitutes for a macOS/Windows lane.
