# ADR-0073 — Keep the plugin SDK in the CLI workspace; defer the repository split

Status: **Accepted** (2026-11-26).
Supersedes [ADR-0072](0072-sdk-independent-repository.md).

## Context

ADR-0072 split `marqdo-plugin-sdk` into the public repository
`cflmy/marqdo-plugin-sdk` and pinned it to one CLI contract `rev`. The
implementation itself was technically sound (clean-clone build, frozen-CLI
integration, no second ABI), but a re-evaluation against the pre-stable project's
architecture showed that the **repository boundary was premature**, even though
the **logical contract boundary is correct and stays**.

The relevant facts at this point in the prerelease line:

- the SDK is a small, contract-only consumer (six source files; dependencies are
  `marqdo-plugin-abi`, `serde`, `serde_json`);
- plugin ABI 1.0, Value Wire 1 and the C header are frozen; the SDK has no
  independent version or release story beyond the ABI generation;
- the C header, `spec/plugin-abi.md`, the `marqdo-plugin-abi` contract, the
  loader/worker and the SDK helpers evolve together and are owned by the same
  maintainers;
- no registry publication or independent maintenance exists for the SDK;
- the CLI product supports external plugin integration without the SDK (C ABI +
  loader are in the CLI), so the SDK is a developer kit, not a runtime feature.

Under issue #13's own criteria, "keep in workspace" is a legal conclusion when
independent consumer/release evidence is absent. The split added a cross-repo
`rev` pin, a second CI lane, a second source location and a documentation
integrity burden (stale in-repo paths, weak entry links) without a current
release or consumer benefit. The CLI's developer-facing contract, header, spec,
SDK and examples should form one atomic change domain.

## Decision

1. **The plugin SDK returns to the CLI workspace** as
   `crates/marqdo-plugin-sdk`, a contract-only crate. It keeps its current clean
   dependency graph: `marqdo-plugin-abi` + `serde`/`serde_json`; no loader,
   worker, host, sandbox or CLI dependency.
2. **The independent repository is deleted.** Its source was restored from CLI
   Git history; no second copy of the SDK or contract remains. The SDK must not
   be reintroduced as a remote repository or as a submodule in this stage.
3. **Logical boundaries stay exactly as ADR-0071 defined them**: the runtime
   depends only on `marqdo-host-contract`; `marqdo-plugin-abi` is a pure
   contract; execution-side loader/worker/manifest/artifact live in
   `marqdo-host::plugin`; the CLI is the single assembly point.
4. **Atomicity is the priority for the pre-stable line.** ABI layout, header,
   spec, contract crate, SDK helpers, fixture and the real CLI invocation test
   land in one commit/PR/lock. There is no cross-repository pin to bump and no
   second current source tree.
5. **External access remains in the CLI product.** The release archive and
   repository carry the C header, public plugin spec, loader/worker and the SDK
   source; plugin authors build their plugin against this repository (or use
   the header from another language). The CLI itself is not compiled by plugin
   users.
6. **Future extraction is a new decision, not an implicit step.** It requires
   new evidence: independent SDK release (for example a published contract
   artifact), a real non-CLI consumer/maintainer, or a distribution need that
   the atomic workspace cannot serve. It then needs a new Accepted ADR and must
   not create a CLI↔SDK gitlink cycle.

## Verification

- SDK crate, fixture, CLI-side SDK gate and workspace/lock are restored; the
  CLI builds and tests them in the same workspace.
- `tools/ci/check_dependency_boundaries.py` enforces the SDK's contract-only
  normal/build dependencies and its presence in the workspace as a reviewed
  crate.
- `tools/ci/check_plugin_sdk.py` builds the SDK fixture with only the public SDK
  and loads it through the real CLI with explicit trust/grant; a tampered
  artifact digest is rejected.
- The remote split repository was deleted after this decision; no temporary
  branch was created for the revert.
- Full evidence: `docs/evidence/2026-11-26-issue-15-sdk-split-reverted.md`.

## Rejected alternatives

- **Keep the split because it was already implemented.** Sunk-cost reasoning;
  it does not satisfy the "necessary split" criterion for this stage.
- **Keep the remote repository as a mirror/copy.** Two current SDK sources and
  a second contract consumer tree are explicitly forbidden; Git history already
  preserves ADR-0072's implementation.
- **Make the SDK a submodule of the CLI.** Inverts the dependency direction and
  risks the CLI gitlink ↔ SDK → CLI contract cycle; the CLI does not consume
  the SDK at build time.
- **Turn the SDK into a prebuilt binary so users need not compile.** Native
  plugins are compiled by their authors by definition; the SDK is a source-level
  helper. A prebuilt SDK is neither a plugin nor a compiler target.

## Consequences

- One atomic language/plugin-development change domain; one lock and one
  candidate SHA for ABI/header/spec/SDK/loader changes.
- External plugin integration keeps working through the CLI's contract and
  loader; the SDK is discoverable in the same public repository as the header
  and spec.
- The split work remains in Git history (ADR-0072, former PR #16) as decision
  history; it is not current architecture.
