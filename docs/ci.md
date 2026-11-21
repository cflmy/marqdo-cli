# CI/CD

## Current setup

- Git remote: `https://git.cflmy.cn/cflmy/marqdo-cli.git`
- Woodpecker: `https://ci.cflmy.cn`
- Pipeline definition: [`.woodpecker.yml`](../.woodpecker.yml)
- Credentials are local-only and ignored; see
  [development-credentials.md](development-credentials.md). Do not put tokens in
  pipeline files or commit messages.

The default Docker pipeline runs on push and pull request:

```text
test:
  rustup component add clippy rustfmt
  apt install curl/ca-certificates + Node.js 22
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace --locked

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
  Bubblewrap/AppArmor namespace support. See [security-vm.md](security-vm.md).
  The Docker pipeline is not an OS-isolation proof.
- **Windows/macOS native lanes**: no runners are configured.
- **Release publication/signing**: no release job is defined.

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
