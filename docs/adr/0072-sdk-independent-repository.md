# ADR-0072 — Independent `marqdo-plugin-sdk` repository

Status: **Accepted** (2026-11-26).

Complements [ADR-0071](0071-engine-host-contract-boundary.md) and implements the
SDK follow-up tracked by issue [#15](https://git.cflmy.cn/cflmy/marqdo-cli/issues/15)
after the contract decoupling of issue #13.

## Context

Issue #13 / ADR-0071 separated the public plugin contract from its execution
side:

- `marqdo-plugin-abi` (CLI repository) now contains only the C ABI v1 layout,
  the wire envelope and protocol errors;
- `dlopen`, manifest/artifact file handling, worker management and invocation
  dispatch live in `marqdo-host::plugin`;
- `marqdo-plugin-sdk` consumed only the contract and was verified by a real
  fixture loaded through the CLI.

The SDK is a leaf with a different consumer group (third-party plugin authors),
a different change cadence and an independent release need. Keeping it in the
CLI workspace made the architecture map imply that the CLI product owns SDK
publication and forced every SDK change through the CLI build. Issue #15
required either fixing the distribution input or recording a blocking ticket.
The CLI repository is public, so a fixed Cargo git `rev` is a fetchable
contract input; there is no need for a submodule, a vendored contract copy or a
second wire implementation.

This is a **deliberate, narrow repository split**, not a mechanical projection
of every crate: engine and native host remain in the CLI workspace (ADR-0071
reopening conditions unchanged), and no empty candidate repository is created.

## Decision

### Topology

| Item | Decision |
|---|---|
| SDK repository | Public `cflmy/marqdo-plugin-sdk` (`main`), own Cargo workspace and lock |
| Contract input | `marqdo-plugin-abi` from `https://git.cflmy.cn/cflmy/marqdo-cli.git` at exact `rev = 2585915e16906dac75f8571bed02e7f6f868c6bc` (the merge commit that landed ADR-0071) |
| CLI dependency on SDK | **None**. CLI normal/build/test lanes do not fetch or build the SDK |
| CLI workspace | `crates/marqdo-plugin-sdk` removed; the SDK is not a workspace member |
| Submodule/gitlink | **Not used** for this split; the SDK leaf must not create a CLI↔SDK identity cycle |
| Acceptance fixture | `tests/fixture` in the SDK repository only; real invocation uses a **frozen released CLI binary** via `tools/check_with_frozen_cli.py` |
| Contract authority | Unchanged: CLI owns the C header, Rust layout, Value Wire and error encodings; the SDK never defines a second contract |
| Publication | SDK releases are versioned independently against a contract generation; bumping the pinned CLI `rev` is a reviewed change |

### Rules

- The SDK repository must resolve and build from a clean clone without the CLI
  checkout; only its fixed git dependency may reach the CLI repository.
- It must not depend on `libloading`, a native host, worker execution, sandbox
  or CLI code.
- The SDK's fixture is a third-party plugin built with only the public SDK. It
  is verified against a frozen CLI archive (same ABI 1.0 contract), not against a
  locally rebuilt CLI.
- The CLI repository must not reintroduce the SDK as a workspace member, a
  copied source tree or a compatibility path; `tools/ci/check_dependency_boundaries.py`
  and `architecture_boundaries.rs` fail if it does.
- The CLI's public contract is authoritative; the SDK repository README and
  AGENTS rules point back to `include/marqdo_plugin_abi_v1.h`,
  `spec/plugin-abi.md` and the pinned commit.
- The existing security statement remains: a C ABI is not a sandbox, and native
  loading requires explicit trust; the SDK grants no capability.

## Verification (candidate)

- SDK repository commit `b31187a` on `main`:
  - clean-clone `cargo build --locked` and `cargo test --locked` pass;
  - the fixture builds with only the SDK;
  - `tools/check_with_frozen_cli.py --marqdo-bin <released marqdo 3.0.0-rc.4>`
    returns `Int 42` and rejects a tampered artifact digest.
- CLI candidate removes the SDK crate, fixture and CLI-side gate; workspace
  tests, architecture boundaries, docs consistency and dependency gates pass;
  the SDK no longer appears in the workspace or lock file.
- Full evidence: `docs/evidence/2026-11-26-issue-15-sdk-independent-repository.md`.

## Rejected alternatives

- **Keep the SDK in the CLI workspace** while calling the contract "decoupled":
  leaves two release cadences and ownership in one repo; issue #15 cannot close.
- **Git submodule/gitlink into the CLI repo**: CLI does not consume the SDK, so
  a submodule would create an unnecessary recursive identity and checkout
  failure mode.
- **Relative path dependency `../cli`**: not an independent build; forbidden by
  issue #13 §6.2.
- **Copy `marqdo-plugin-abi`/Value Wire into the SDK repo**: would create a
  second contract and wire encoder authority.
- **Publish the contract to a registry first**: no registry/release channel is
  approved here; the public git `rev` is the available precise input. A future
  published artifact can replace the git `rev` without changing authority.

## Consequences and rollback

- The architecture map now shows one CLI contract producer and one independent
  SDK consumer; CLI CI no longer carries SDK-only checks, and SDK CI owns them.
- The SDK remains pinned to a CLI merge commit. Changing the contract or
  advancing the pin is a deliberate cross-repository change with its own
  evidence; a contract change must land in the CLI first, then the SDK pin is
  bumped.
- Rollback is a complete known-good combination: keep the CLI commit and the SDK
  pin that were verified together; do not restore a copied SDK or an old ABI.
- External SDK consumers can fetch the public CLI contract today. If a
  registry-published contract or a different visibility policy is later chosen,
  that is a new ADR, not an implicit change.
