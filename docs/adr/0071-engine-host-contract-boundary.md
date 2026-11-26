# ADR-0071 — Engine / Host Contract / Native Host / CLI boundaries

Status: **Accepted** for the pre-stable 3.0 line (2026-11-26).

Supersedes the historical review-RFC position that a Host contract crate is a
second-step, optional move. It does not supersede the language contract
(ADR-0052 / RFC-LANG-0002), the resource/operation ownership rules (ADR-0062),
the foreign F-A boundary (ADR-0063), the single-contract policy (ADR-0068) or
the explicit stdlib Bundle build input (ADR-0070 where landed). It replaces only
the old architecture-gate constraints that asserted `marqdo-runtime ->
marqdo-host` and a frozen crate count.

## Context

Issue #13 audited the tree at `4bbefc9…` and found:

- the runtime linked the native host (`marqdo-host`) even though the interpreter
  already accepted `&mut dyn Host`;
- the `Host` trait, `HostOp` and `StdHost` lived in one module, and
  `ResourceRead` sat beside real file/TCP/TLS implementations;
- the plugin ABI crate contained the C contract, the wire envelope, manifest
  validation, artifact selection, `dlopen` and the worker process, so the
  third-party SDK transitively carried execution-side dependencies;
- old architecture gates froze sixteen crate names and the
  runtime-to-native-host edge, so the intended boundary could not be tested;
- historical planning documents competed with the current overview as a source
  of truth.

The same audit fixed the scope: one language implementation, one contract per
public boundary, no second interpreter, no new runtime protocol, no mandatory
remote repository split, and no empty candidate repositories. Missing
independent-consumer evidence means "keep in the workspace" is the correct
repository decision.

## Decision

### Responsibilities

| Layer | Owns | Must not own |
|---|---|---|
| Engine | Span, Value, expression, the one Markdown profile, IR, frontend mapping/static binding, Runtime evaluation and `Memory`, and the neutral Host contract | concrete OS I/O, sandbox launch, plugin loading, project filesystem, CLI commands |
| `marqdo-host-contract` | `Host` trait, `HostOp`, `ExecutionContext`, `ResourceRead`, `FrameId`, `ParameterSource`, `LoopTransfer`, `RuntimeObservation` | native implementation, grant policy, the public Observation wire encoding, any wire/RPC protocol |
| Native Host (`marqdo-host` and its sandbox/foreign support crates) | capability broker, resources and handles, OS adapters, budgets/deadlines, observation sink, trusted plugin/provider/foreign execution | language parsing/evaluation, a second static binder, CLI projection |
| `marqdo-plugin-abi` | C ABI v1 layout/assertions, wire envelope and protocol errors | `dlopen`, worker/process management, artifact/manifest file execution, capability grants |
| `marqdo-host::plugin` | manifest/artifact validation, native loading, worker process, invocation dispatch | a second ABI or wire definition |
| `marqdo-plugin-sdk` | third-party developer helpers over the public contract | host, loader, CLI, worker execution, sandbox |
| CLI | arguments, single `project_prepare`, command routing, product Analysis/Observation projection, Tool install/dispatch, Host+Runtime assembly | language evaluation, domain algorithms, a second binding path |

### Dependency rules

- `marqdo-runtime -> marqdo-host-contract` only. The engine crates must not reach
  `marqdo-host`, sandbox, foreign, plugin ABI/SDK, worker, package, CLI,
  `libloading`, `rustls` or other native/domain dependencies on normal or build
  edges.
- `marqdo-host-contract -> marqdo-value, marqdo-span` only.
- The native host may implement the contract and consume the public ABI, but it
  must not depend back on runtime, frontend, IR or the CLI.
- The CLI is the only composition root allowed to combine engine + native host +
  package. `check`/`inspect` never load a native library, start a worker or
  access the network.
- `marqdo-plugin-sdk` consumes only `marqdo-plugin-abi`; it must not depend on a
  loader, host, worker process, sandbox or CLI.
- There is no second `Host`/`HostOp`, no second Value encoding, no
  `EngineValue`/`HostValue`/`WireIR`, no HTTP/RPC/JSON handshake for the
  engine-to-host edge and no `legacy` feature. The Rust trait is an internal
  compile-time interface inside one codebase.

### Invariants preserved (not moved, not weakened)

- Fuel, stack depth, deadline, output budget and `Memory` limits stay owned and
  checked exactly where they were; replacing the host must not remove a check.
  The contract carries the calls, not platform-neutral defaults that silently
  grant unlimited budgets.
- Capability grants, resource identity/handles, cleanup, provider registration,
  strict isolation, plugin trust, foreign supervision and observation privacy
  stay owned by the native host and CLI. `check`/`inspect` stay non-executing.
- Static binding remains single: one registry, one `bind_program_calls`, one
  `CallBindingTable`; the runtime never re-resolves names.
- Public protocols are unchanged: Analysis revision 3, Observation revision 1,
  Tool revision 1, Plugin ABI 1.0, Value Wire 1, Provider v2, lock v4. Moving
  Rust modules is not a protocol change.
- Stdlib runtime contract is unchanged. Phase A/B of issue #11/#12 remain a
  separate delivery; the CLI does not gain a second Bundle format or a dev
  fallback authority.

### Repository decision

The Engine and Native Host stay in this workspace. Issue #13 §9.2 admission is
not met: there is no real external Engine consumer or independent platform
maintainer, and an empty candidate repository would be a prohibited boundary
violation. This ADR records **keep in workspace** as the legal conclusion and
names the reopening conditions in `docs/architecture/CURRENT.md` §7.

The plugin SDK's physical move is a follow-up: the contract is decoupled now,
and the move requires a clean-directory build against a fixed contract revision
(issue #13 S07/S08). No gitlink/submodule or empty `marqdo-cli-*` repository is
created by this ADR.

## Verification

The acceptance gates are executable, not documentary:

- `tools/ci/check_dependency_boundaries.py` reads Cargo metadata for the
  workspace and the independent consumer, checks the reviewed crate allowlist and
  the forbidden transitive edges, and is itself covered by
  `tools/ci/test_dependency_boundaries.py` negative cases.
- `tests/embedding` is a separate workspace and lock file; it executes a real
  `.mq.md` through Markdown → frontend → static binding → runtime with a bounded
  test host, and rejects fuel/stack/deadline/memory/resource violations.
- `tools/ci/check_engine_boundary.py` builds and runs that consumer, builds the
  real CLI, executes the same file and compares terminal Value Wire results, and
  checks the runtime build tree.
- `tools/ci/check_plugin_sdk.py` builds the SDK fixture with the public SDK and
  loads it through the real CLI with explicit trust/grant, then rejects a
  tampered artifact digest.
- `cargo test -p marqdo-cli --test architecture_boundaries` keeps the semantic
  gates (single Host trait, plugin contract split, engine-only consumer, reviewed
  HostOp/Value/builtin/resource-operation catalogues) and the existing security
  and product checks.
- Evidence for the candidate commit lives in
  `docs/evidence/2026-11-26-issue-13-engine-host-boundary.md` and
  `docs/STATUS.md`. Platform lanes not executed here stay `UNVERIFIED`; no old
  release evidence is reused as a new PASS.

## Rejected alternatives

- **Create `marqdo-engine`/`marqdo-host` remote repositories now.** No real
  consumer, no ownership/release boundary, prohibited empty candidates; would
  also create a cross-repository atomic-semantics tax.
- **Keep the runtime linked to `marqdo-host` and only rename the crate.** Does
  not close the dependency edge; an embedded consumer would still link native
  TLS, loading and process code.
- **Move the whole `observe` module or CLI projection into the contract.** The
  interpreter needs only the emitted facts; the sink, redaction and public
  envelope belong to the host/CLI, and moving them would create a second
  protocol centre.
- **Leave manifest/artifact in the ABI crate.** The SDK would transitively keep
  `serde_yaml`/digest/native-adjacent code and the contract crate would again own
  execution-side choices.
- **Add a compatibility re-export of the old `marqdo_host::{Host, ...}` paths.**
  Creates a second active path; all in-tree consumers are updated atomically.
- **Loosen the old gates instead of replacing them.** Security, product,
  protocol and negative tests are retained; only the frozen crate-name and
  runtime-to-native-host assertions are replaced by the responsibility graph.

## Consequences

- An independent consumer can build and compute without the native host or CLI.
- Adding a host capability does not change engine code; adding an engine semantic
  change does not change host code.
- The workspace gains one small contract crate and one plugin module move; crate
  count is explicitly not a KPI.
- Unfinished ecosystem work (stdlib Phase B, SDK physical repository, regex/
  compression migration, foreign F-B, WASM probe, Windows/macOS/strict evidence)
  remains open and honestly recorded; none of it is required to close the core
  M0–M4 boundary work.
