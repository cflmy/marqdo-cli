# Marqdo CLI — next-generation design starter

> **Design-only starter package. This archive does not contain an executable CLI or a completed Rust implementation.**

Marqdo aims to be a **Markdown-native, independently interpreted programming language**: Markdown AST structures carry program control semantics; readable prose can coexist with execution, while runtime effects are explicit and controlled. The new toolchain is independent of the original `cflmy/marqdo` repository.

## Read first

- [`docs/DEVELOPMENT_BLUEPRINT.md`](docs/DEVELOPMENT_BLUEPRINT.md) — the comprehensive Chinese design and development specification: motivation, formal mapping, functions/parameters/calls/returns, execution, references, Math/Foreign kernels, environment and package manager, sandbox, plugin ABI/SDK, milestones, tests and related projects.
- [`AGENTS.md`](AGENTS.md) — succinct onboarding instructions for coding agents and new contributors.

## Planned CLI surface

```bash
marqdo init
marqdo run index.mq.md
marqdo call index.mq.md main
marqdo check index.mq.md
marqdo inspect index.mq.md --ast
marqdo add <package>
marqdo lock && marqdo sync
marqdo plugin inspect <artifact>
```

These commands are **planned APIs**, not yet available in this starter package. Begin by implementing P0 tests and a reference Markdown AST explorer; do not start with a monolithic CLI or port old parser heuristics.

## Design status

The language direction and architecture principles have been selected. Detailed proposals still require ADRs and conformance testing before a stable version. See blueprint section 17 for unresolved items and section 18 for the Alpha definition of done.

## References

- [CommonMark](https://spec.commonmark.org/0.31.2/)
- [GFM](https://github.github.io/gfm/)
- [Comrak](https://comrak.ee/)
- [Cargo](https://doc.rust-lang.org/cargo/)
- [uv](https://docs.astral.sh/uv/)
- [Wasmtime Component Model](https://component-model.bytecodealliance.org/)
- [Legacy Marqdo source](https://github.com/cflmy/marqdo)
