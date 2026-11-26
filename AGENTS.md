# Marqdo CLI — Agent / Contributor Operating Rules

**Start here:** read `docs/architecture/RFC-LANG-0002-unified-semantics.md` — it is the single current language authority — and its normative summary `spec/mapping.md` + `spec/expression.ebnf`. Then read `docs/architecture/CURRENT.md` (the single current architecture overview, including the Engine / Host Contract / Native Host map), [ADR-0071](docs/adr/0071-engine-host-contract-boundary.md) (boundary rules and keep-in-workspace decision), `docs/STATUS.md` (what is actually implemented), the relevant `spec/` files, `docs/adr/` for decisions at their date, the developer entry point `docs/developers/README.md`, `docs/contributor-guide.md`, and `tests/README.md` plus the conformance fixtures. Do not assume any current implementation, legacy Marqdo syntax, historical planning document or old architecture gate is authoritative.

For next-phase work, the authoritative release policy for the current
3.0 prerelease line is `docs/MARQDO_3_0_PRERELEASE_PLAN.md` (beta gates,
REL-01..REL-05, version axes, LTS conditions). The current architecture and
dependency direction are owned by `docs/architecture/CURRENT.md` and
[ADR-0071](docs/adr/0071-engine-host-contract-boundary.md). The historical plans
(`DEVELOPMENT_BLUEPRINT.md`, `NEXT_PHASE_IMPLEMENTATION.md`,
`STABLE_RELEASE_IMPLEMENTATION.md`, `CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`,
`EXTENSIBILITY_AUDIT.md`, `NEXT_RELEASE_CI_CONVERGENCE.md`) are archived design
history: read them with that status, do not use them as current requirements, and
do not create a second running-status page. `COMPATIBILITY.md` and
`docs/security-gate.md` remain current inputs. None describes implemented
behavior; re-check against HEAD and tests. `strict` remains fail-closed until
G-C passes.

The stable-release spec also governs scope: do not add a top-level CLI command,
Knowledge/Proof/View special-casing, a second interpreter or a new package
registry. Fix the current Windows/macOS CI behavior, protocol/environment
correctness and release evidence before expanding the surface.

## Non-negotiable architectural constraints

1. New independent language/toolchain. **Do not silently modify `cflmy/marqdo`** or claim compatibility with its syntax, package format or plugin ABI.
2. CommonMark/GFM/Math AST is the sole Markdown syntax structure. Do **not** implement a second Markdown parser or use regex heuristics to decide whether text is executable.
3. The language has **one** Markdown→program mapping (RFC-LANG-0002): `@exec`/`@knowledge` heading modes with ancestor inheritance; only an Exec H2 is callable; `Strong`, `Emph` and inline `Code` are interchangeable one-statement payload carriers; `if`/`else`/`while`/`end`/`return`/`params` decide control flow; lists, block quotes and paragraphs are presentation containers with no instruction type. Invalid payloads are hard errors, never prose fallbacks. Do not reintroduce Strong=execute, Emph=return, ordered-list=branch, bullet-list=loop or block-quote=call, and do not add a per-file syntax switch, `map_v1`, `--legacy` or a second active spec.
4. Prose and executable payloads may mix freely in one function/branch/loop body. Prose itself never causes an action; executable payloads with no owning H2 Exec function are errors (`sem.exec_without_function`).
5. Marqdo is executed by **its own Rust interpreter**. Python/Node are optional foreign kernels, never the official compiler target.
6. Code fences and math expressions create inert native `Code` and `Formula` values. Evaluation requires explicit calls and relevant capability grants. Inline `Code` spans are payload carriers only in an Exec scope; fenced code is never executed.
7. YAML project/dependency and frontmatter imports must be explicit. Markdown Links are static references, never calls or imports. Calls are expressions inside executable payloads.
8. Third-party plugins require a versioned **C ABI and SDK**. A C ABI is not a sandbox. Default third-party execution must be isolated; in-process native loading requires explicit trust.
9. Side effects go through `CapabilityHost`; do not invent host bypasses for tests, plugins, math or foreign kernels. `check` and `inspect` cannot execute untrusted project code.
10. Language semantics, package format, ABI, authorization defaults or public CLI behavior changes require an ADR + tests + spec updates.
11. RFC-LANG-0002 is implemented by the frontend, `marqdo-expression`, IR lowering, runtime and the bundled stdlib. Keep `spec/`, implementation, fixtures, stdlib digest and docs in lockstep; there is no legacy mapper or per-file syntax switch to fall back to. Do not make a release claim beyond the evidence recorded in `docs/STATUS.md`.
12. The language engine depends on the neutral `marqdo-host-contract` only; it must never link `marqdo-host`, the sandbox, foreign kernels, the plugin loader/worker, package or CLI in normal/build dependencies. `marqdo-host-contract` depends on `Value`/`Span` only. The native host implements the contract and must not depend back on the runtime/frontend/IR. The plugin ABI crate is a contract (no `dlopen`/worker/manifest execution); the SDK consumes the contract only. CLI is the one assembly point. This is enforced by `tools/ci/check_dependency_boundaries.py`, `tools/ci/check_engine_boundary.py`, `tools/ci/check_plugin_sdk.py` and `crates/cli/tests/architecture_boundaries.rs`; see ADR-0071.
13. Do not create empty candidate repositories (for example `marqdo-engine`/`marqdo-host`/`marqdo-cli-*`) to "solve" architecture. A repository split requires the issue #13 §9.2 evidence and a separate accepted ADR; "keep in workspace" is a valid recorded decision. The plugin SDK stays in this CLI workspace as a contract-only crate (ADR-0073); do not re-create an independent SDK repository, mirror or submodule for the pre-stable line. Do not add a second Host trait, Value encoding, wire/RPC engine protocol or compatibility re-export path.

## Pre-stable protocol changes

Read [ADR-0068](docs/adr/0068-single-current-public-contracts.md) and the
[single-contract convergence policy](docs/architecture/PROTOCOL_CONVERGENCE_POLICY.md)
before changing a public boundary. Keep one current specification, producer and validator per
boundary. Replace retired paths in the same integration, reject unsupported
formats explicitly, and update independent consumers and release smoke tests.
Keep old formats in Git history. Preserve authority, attribution and resource
limits during refactors. Confirm cross-repository and release evidence before
closing a protocol replacement issue.

## Development workflow

1. Identify the affected rule and cite RFC-LANG-0002 / `spec/mapping.md` (or, for architecture-only changes, the relevant `docs/DEVELOPMENT_BLUEPRINT.md` section).
2. Update the normative spec **in place** first; there is never a second active syntax spec.
3. Add a small `.mq.md` positive/negative conformance fixture and define the expected Markdown AST, execution IR and diagnostics.
4. Implement only the smallest module that satisfies this fixture. Keep Markdown parser, semantic mapper, expression parser, runtime and host separated.
5. Run formatting, clippy, unit tests, golden tests, the architecture/dependency gates and relevant security/ABI compatibility tests (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p marqdo-cli`, `python3 tools/ci/check_dependency_boundaries.py`, `python3 tools/ci/check_engine_boundary.py`, `python3 tools/ci/check_plugin_sdk.py`).
6. Write the tradeoff and state of completion in the PR. Never say a feature is finished because a design example looks executable. Update `spec/` and `docs/STATUS.md` when behavior or status changes; the docs describe what the code does and flag divergences explicitly.

## Unresolved design topics

RFC-LANG-0002 resolves the old emphasis/control-role conflict: `Strong`, `Emph` and inline `Code` are equivalent carriers and controls are keywords with explicit `end`. Remaining topics still need separate Accepted ADRs (parameter inference policy, plugin memory ownership, strict platform isolation, future `elif`/`for`/`break`, knowledge anchors). Do not freeze them through undocumented code.

## Priorities

P0 language contract maintenance (RFC-LANG-0002, single grammar, corpus and stdlib) → P1 independent interpreter → P2 references/CLI → P3 package/env → P4 sandbox → P5 math/foreign → P6 plugin ABI/SDK → P7 performance/release. Design the Host security boundary and ABI early, even if full implementation arrives later.

**Never** translate Marqdo into Python as the official execution method; **never** silently downgrade an invalid payload to prose, a requested strict sandbox to an unrestricted process, or the current syntax to a legacy mapper.
