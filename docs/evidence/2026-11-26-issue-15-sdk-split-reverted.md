# Evidence — issue #15 plugin SDK split reverted to the CLI workspace

Date: 2026-11-26 (Asia/Shanghai)
Tracking issue: <https://git.cflmy.cn/cflmy/marqdo-cli/issues/15>
Decision: [ADR-0073](../adr/0073-sdk-stays-in-cli-workspace.md) (supersedes
[ADR-0072](../adr/0072-sdk-independent-repository.md))
Parent: issue #13 / [ADR-0071](../adr/0071-engine-host-contract-boundary.md)

## Decision

Parent CLI contract merge: `2585915e16906dac75f8571bed02e7f6f868c6bc`.
Deleted split repository commits: `b31187a` / `c3134e4` (kept in Git history
of the former repository only; no source copy remains).

The plugin SDK stays in the CLI workspace as a contract-only crate. The public
repository `cflmy/marqdo-plugin-sdk` created by ADR-0072 was deleted; its source
was restored from CLI Git history. The pre-stable line prioritizes atomic
ABI/header/spec/SDK/loader changes in one workspace, lock and candidate SHA.
The logical ADR-0071 boundaries are unchanged.

## Actions executed

| Step | Result |
|---|---|
| Restore `crates/marqdo-plugin-sdk` from CLI history | Restored contract-only crate (`marqdo-plugin-abi` + serde/serde_json); no loader/host/worker dependency |
| Restore workspace member and lock | `cargo check --workspace --all-targets --locked` resolves the SDK in the workspace; `Cargo.lock` contains it again |
| Restore `tests/sdk-plugin` and `tools/ci/check_plugin_sdk.py` | Real fixture/gate back in the CLI; builds only against the public SDK and invokes via the real CLI |
| Restore architecture/dependency/docs references | `architecture_boundaries.rs`, `check_dependency_boundaries.py`, `docs_consistency` paths, README/AGENTS/spec/developer guides point at the in-repo crate again |
| Delete remote `cflmy/marqdo-plugin-sdk` | Authorized by the maintainer; the repository and its branches are gone, no mirror remains |
| No temporary branch | The revert is committed directly on `master` and pushed; old merged feature branches are deleted separately |
| Record superseded history | ADR-0072 and the old issue-#15 evidence carry Superseded banners; ADR-0073 is the current decision |

## Verification (local, Linux x86_64, Rust 1.100)

| Gate | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check` | PASS |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| Rust tests | `cargo test --workspace --locked --offline --no-fail-fast` | PASS — **636 passed / 0 failed / 2 ignored** (same as the pre-split workspace) |
| Architecture boundaries | `cargo test -p marqdo-cli --test architecture_boundaries --locked` | PASS |
| Docs consistency | `cargo test -p marqdo-cli --test docs_consistency --locked` | PASS |
| Dependency direction | `python3 tools/ci/check_dependency_boundaries.py` | PASS — SDK back in the reviewed workspace allowlist with contract-only edges |
| SDK real consumption | `python3 tools/ci/check_plugin_sdk.py` | PASS — builds fixture with only the SDK; real CLI returns `Int 42`; tampered digest rejected |
| Engine differential | `python3 tools/ci/check_engine_boundary.py` | PASS |
| Tools unit tests | `python3 -m unittest discover -s tools/ci -p "test_*.py"` | PASS |

Remote CI on the code commit `98d256f`: Woodpecker push pipeline **#112**
success (contract/test/minimum-rust/build) and Gitea commit status `success`.
This evidence-only follow-up does not change code, lock or protocol bytes. No
split repository, mirror or submodule exists after the deletion.

## Current architecture facts

- `marqdo-plugin-abi` is still the pure public contract; loader/worker/manifest/
  artifact remain in `marqdo-host::plugin` (ADR-0071).
- `marqdo-plugin-sdk` is a CLI workspace member with normal/build dependencies
  only on the contract + serde; the CLI product is the only assembly point.
- Plugin authors get header, spec, SDK, examples and loader from one public CLI
  repository/release; they compile their plugin, not the CLI.
- The CLI runtime does not depend on the SDK; external plugin loading remains
  available (C ABI header + `--plugin`/`--trust-native-plugin`/`--allow-plugin`
  + worker transport).
