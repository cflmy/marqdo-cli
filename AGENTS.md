# Marqdo CLI — Agent / Contributor Operating Rules

**Start here:** read `docs/architecture/RFC-LANG-0002-unified-semantics.md` — it is the single current language authority — and its normative summary `spec/mapping.md` + `spec/expression.ebnf`. Then read `docs/DEVELOPMENT_BLUEPRINT.md` (architecture baseline; its language-mapping sections are historical), `docs/STATUS.md` (what is actually implemented, including the in-progress syntax migration), the relevant `spec/` files, `docs/adr/` for decisions at their date, `docs/contributor-guide.md`, and `tests/README.md` plus the conformance fixtures. Do not assume any current implementation or legacy Marqdo syntax is authoritative.

For next-phase work, the single authoritative plan is
`docs/STABLE_RELEASE_IMPLEMENTATION.md` (stable-release gates G-A/G-B/G-C,
PR order and release checklist). Also read `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`
(historical stabilization spec, baseline `0adebe4`), `docs/NEXT_PHASE_IMPLEMENTATION.md`
(M0.6 plan), `docs/EXTENSIBILITY_AUDIT.md` (E01–E16), `COMPATIBILITY.md` and
`docs/security-gate.md`. The older plans are historical input: the stable-release
spec supersedes them on current baseline, gap list, PR order and release gates.
None describes implemented behavior; re-check against HEAD and tests. `strict`
remains fail-closed until G-C passes.

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
11. The current tree is **spec-first**: RFC-LANG-0002 and `spec/` describe the target language, while the Rust frontend, expression parser and bundled stdlib still implement the pre-RFC mapping. Do not claim the new syntax is implemented or make a release claim until the migration is complete and its gates are green; keep `docs/STATUS.md` honest.

## Development workflow

1. Identify the affected rule and cite RFC-LANG-0002 / `spec/mapping.md` (or, for architecture-only changes, the relevant `docs/DEVELOPMENT_BLUEPRINT.md` section).
2. Update the normative spec **in place** first; there is never a second active syntax spec.
3. Add a small `.mq.md` positive/negative conformance fixture and define the expected Markdown AST, execution IR and diagnostics.
4. Implement only the smallest module that satisfies this fixture. Keep Markdown parser, semantic mapper, expression parser, runtime and host separated.
5. Run formatting, clippy, unit tests, golden tests and relevant security/ABI compatibility tests (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p marqdo-cli`).
6. Write the tradeoff and state of completion in the PR. Never say a feature is finished because a design example looks executable. Update `spec/` and `docs/STATUS.md` when behavior or status changes; the docs describe what the code does and flag divergences explicitly.

## Unresolved design topics

RFC-LANG-0002 resolves the old emphasis/control-role conflict: `Strong`, `Emph` and inline `Code` are equivalent carriers and controls are keywords with explicit `end`. Remaining topics still need separate Accepted ADRs (parameter inference policy, plugin memory ownership, strict platform isolation, future `elif`/`for`/`break`, knowledge anchors). Do not freeze them through undocumented code.

## Priorities

P0 language migration to RFC-LANG-0002 (frontend, expression parser, stdlib, fixtures) → P1 independent interpreter → P2 references/CLI → P3 package/env → P4 sandbox → P5 math/foreign → P6 plugin ABI/SDK → P7 performance/release. Design the Host security boundary and ABI early, even if full implementation arrives later.

**Never** translate Marqdo into Python as the official execution method; **never** silently downgrade an invalid payload to prose, a requested strict sandbox to an unrestricted process, or the current syntax to a legacy mapper.
