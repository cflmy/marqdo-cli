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

The default Docker pipeline runs on push and pull request:

```text
test:
  rustup component add clippy rustfmt
  apt install curl/ca-certificates/python3 + Node.js 22
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace --locked
  python3 -m unittest discover -s tools/ci -p "test_*.py"

build (depends on test):
  cargo build --release --locked -p marqdo-cli
```

Node.js 22 is required because the optional foreign-kernel tests use ES-module
syntax; Debian bookworm's Node 18 fails those tests.

> **Language contract note.** `cargo test --workspace` includes conformance
> fixtures that execute `.mq.md` source under
> [RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md). The
> implementation, stdlib digest and corpus are green at `ffd1cc9`; a red
> executable-source suite is a real regression, not an expected migration
> state. Do not "fix" it by restoring the old mapper or regenerating goldens;
> see [`STATUS.md`](STATUS.md). The pipeline definition itself
> (fmt/clippy/test/build, security lane, platform gaps) is unchanged.

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
