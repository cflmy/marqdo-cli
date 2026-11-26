# Evidence — issue #15 independent `marqdo-plugin-sdk` repository

Date: 2026-11-26 (Asia/Shanghai)
Tracking issue: <https://git.cflmy.cn/cflmy/marqdo-cli/issues/15>
Decision: [ADR-0072](../adr/0072-sdk-independent-repository.md)
Parent: issue #13 / [ADR-0071](../adr/0071-engine-host-contract-boundary.md)

## Topology and identity

| Object | Exact identity |
|---|---|
| SDK repository | `cflmy/marqdo-plugin-sdk` (public), `main` at `b31187a` |
| CLI contract input | `marqdo-plugin-abi` from `https://git.cflmy.cn/cflmy/marqdo-cli.git` `rev = 2585915e16906dac75f8571bed02e7f6f868c6bc` (merge of PR #14) |
| CLI candidate | `architecture/sdk-independent-repository`; PR/CI SHA recorded by the PR |
| Frozen CLI acceptance binary | `marqdo` 3.0.0-rc.4 Linux x86_64 release archive, SHA-256 from the release `.sha256` asset |
| ABI | Plugin ABI 1.0 / Value Wire 1 / package + provider formats unchanged |

The SDK repository is its own Cargo workspace and lock. The CLI does not depend
on it, and the SDK does not depend on the CLI checkout, the native host, a
loader/worker, a sandbox or the CLI product.

## SDK repository verification (clean clone, independent of CLI checkout)

Commands run in a fresh `git clone https://git.cflmy.cn/cflmy/marqdo-plugin-sdk.git`
outside this repository:

| Gate | Command | Result |
|---|---|---|
| Locked build | `cargo build --locked` | PASS (contract fetched from the pinned public CLI git `rev`) |
| SDK tests | `cargo test --locked` | PASS (4 unit tests + 1 doc-test, 2 ignored doc examples) |
| Third-party fixture | `cargo build --locked --manifest-path tests/fixture/Cargo.toml` | PASS (built only from the SDK) |
| Frozen CLI invocation (S04/S07) | `python3 tools/check_with_frozen_cli.py --marqdo-bin frozen/marqdo` | PASS — real CLI returns `Int 42` |
| Tampered artifact (S05/S06) | same script, `sha256` replaced | PASS — rejected with artifact/sha256 diagnostic |

The frozen binary was the released 3.0.0-rc.4 archive, not a locally rebuilt CLI;
the plugin ABI is unchanged across the #13 decoupling, which is exactly the
cross-repository compatibility being tested.

SDK repository CI (`.woodpecker/main.yml`) repeats the workspace checks, the
fixture build and the frozen-CLI integration after downloading that released
archive. Enabling the Woodpecker project/webhook for the new repository is a
repository-administration step outside this change; the pipeline definition is
checked in and the same commands were executed manually above.

## CLI repository verification (this change)

| Gate | Command | Result |
|---|---|---|
| Workspace resolution | `cargo check --workspace --locked` | PASS; `marqdo-plugin-sdk` no longer appears in `Cargo.lock` |
| Formatting | `cargo fmt --all -- --check` | PASS |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| Rust tests | `cargo test --workspace --locked --no-fail-fast` | PASS — 631 passed / 0 failed / 0 ignored (the removed SDK 4 unit + 1 doc test now run in the SDK repository; the two previously ignored SDK doc examples moved with them) |
| Architecture boundaries | `cargo test -p marqdo-cli --test architecture_boundaries --locked` | PASS — SDK absence/moved-repository assertion |
| Docs consistency | `cargo test -p marqdo-cli --test docs_consistency --locked` | PASS |
| Dependency direction | `python3 tools/ci/check_dependency_boundaries.py` | PASS — SDK no longer a workspace member; negative case added |
| Tools unit tests | `python3 -m unittest discover -s tools/ci -p "test_*.py"` | PASS |
| Engine differential | `python3 tools/ci/check_engine_boundary.py` | PASS (unchanged) |

Removed from the CLI workspace/repository in the same integration:

- `crates/marqdo-plugin-sdk/` (moved to the independent repository);
- `tests/sdk-plugin/` (fixture moved to `tests/fixture` in the SDK repository);
- `tools/ci/check_plugin_sdk.py` (its superset lives in the SDK repository and
  uses a frozen CLI binary);
- workspace/lock entry, architecture/layer docs, CI lane entries and current
  developer references.

## Not verified / follow-ups

- **Woodpecker activation** for the new public repository is administrative; the
  pipeline definition is committed, but no remote pipeline run is claimed here.
- **Registry publication** of the contract crate is not introduced; the public
  CLI git `rev` is the precise, fetchable input. A future published artifact
  would be a new decision and would not change contract authority.
- Engine/Native Host remain in the CLI workspace under ADR-0071; this split does
  not reopen that decision.
- No second ABI, wire encoder, loader or host copy exists in the SDK repository;
  no compatibility/submodule path was added to the CLI repository.
