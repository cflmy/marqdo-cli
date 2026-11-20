# Marqdo CLI — Agent / Contributor Operating Rules

**Start here:** read `docs/DEVELOPMENT_BLUEPRINT.md` before proposing code. It is the project architecture baseline. Then read the relevant `spec/` files and conformance fixtures. Do not assume any current implementation or legacy Marqdo syntax is authoritative.

## Non-negotiable architectural constraints

1. New independent language/toolchain. **Do not silently modify `cflmy/marqdo`** or claim compatibility with its syntax, package format or plugin ABI.
2. CommonMark/GFM/Math AST is the sole Markdown syntax structure. Do **not** implement a second Markdown parser or use regex heuristics to decide whether text is executable.
3. Executable roles are based on AST type and valid structural context: `Strong` execution, `Emph` return, ordered list branch, bullet list loop, blockquote invocation, heading scoping. Invalid executable shapes are compile errors, not ordinary prose fallbacks.
4. Prose and executable nodes may mix freely in one function/branch/loop body. Prose itself never causes an action.
5. Marqdo is executed by **its own Rust interpreter**. Python/Node are optional foreign kernels, never the official compiler target.
6. Code fences and math expressions create inert native `Code` and `Formula` values. Evaluation requires explicit calls and relevant capability grants.
7. YAML project/dependency and frontmatter imports must be explicit. Markdown Links are static references, not implicit calls or imports.
8. Third-party plugins require a versioned **C ABI and SDK**. A C ABI is not a sandbox. Default third-party execution must be isolated; in-process native loading requires explicit trust.
9. Side effects go through `CapabilityHost`; do not invent host bypasses for tests, plugins, math or foreign kernels. `check` and `inspect` cannot execute untrusted project code.
10. Language semantics, package format, ABI, authorization defaults or public CLI behavior changes require an ADR + tests + spec updates.

## Development workflow

1. Identify the affected rule and cite the section of `docs/DEVELOPMENT_BLUEPRINT.md`.
2. Add a small `.mq.md` positive/negative conformance fixture and define the expected Markdown AST and execution IR.
3. Implement only the smallest module that satisfies this fixture. Keep Markdown parser, semantic mapper, runtime and host separated.
4. Run formatting, clippy, unit tests, gold tests and relevant security/ABI compatibility tests.
5. Write the tradeoff and state of completion in the PR. Never say a feature is finished because a design example looks executable.

## Unresolved design topics

The blueprint's section **17** lists issues that need separate Accepted ADRs (e.g. emphasis conflicts, parameter inference ambiguity, plugin memory ownership, strict platform isolation). Use its proposed baseline for prototypes, but do not freeze these topics through undocumented code.

## Priorities

P0 AST/grammar contracts → P1 independent interpreter → P2 references/CLI → P3 package/env → P4 sandbox → P5 math/foreign → P6 plugin ABI/SDK → P7 performance/release. Design the Host security boundary and ABI early, even if full implementation arrives later.

**Never** translate Marqdo into Python as the official execution method; **never** silently downgrade an invalid execution node to prose or a requested strict sandbox to an unrestricted process.
