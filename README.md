# Marqdo CLI

Marqdo is a **Markdown-native, independently interpreted programming language**:
standard CommonMark/GFM structures carry program control (headings scope,
`**strong**` executes, `*emph*` returns, ordered lists branch, bullet lists
loop, block quotes call), prose and executable nodes mix in one document, and
all side effects go through a capability-checked host boundary. The official
implementation is a Rust workspace; Python/Node are optional foreign kernels,
never the compiler target.

This repository is the new, independent Marqdo toolchain (M0.5). It is **not**
a compatibility rewrite of `cflmy/marqdo`; the old syntax, package format and
plugin ABI v2 are not claimed to work here.

> **Status: working implementation, not an Alpha release.** P0/P1 and the
> documented Math/Foreign subset are implemented and tested; modules/package/
> security/plugin support is functional but partial; strict isolation, worker
> plugins, git packages and performance/release work are not done. See
> [`docs/STATUS.md`](docs/STATUS.md) and "Deliberate limits" below. The M0.6
> extensibility plan in
> [`docs/NEXT_PHASE_IMPLEMENTATION.md`](docs/NEXT_PHASE_IMPLEMENTATION.md) is a
> **Proposed** next-phase design, not an implemented feature list.

## Build

Requires Rust 1.82+.

```bash
cargo build -p marqdo-cli
# binary: target/debug/marqdo  (or $CARGO_TARGET_DIR/debug/marqdo)
```

## Quickstart

```bash
marqdo init demo
cd demo
marqdo check index.mq.md
marqdo run index.mq.md            # executes ## main
marqdo inspect index.mq.md --ast
marqdo inspect index.mq.md --signature
marqdo call index.mq.md main
```

Example project from scratch (`index.mq.md`):

````markdown
# 数学

## 阶乘

1. `n <= 1`

   *1*

2. `n > 1`

   **结果 = n * 阶乘(n=n-1)**

   *结果*
````

```bash
marqdo check examples/factorial.mq.md
marqdo call examples/factorial.mq.md 数学.阶乘 --arg n=5   # 120
```

The parser is frozen: `marqdo.md.v1` = CommonMark 0.31.2 + GFM tables,
strikethrough, task lists + `$...$`/`$$...$$` math (comrak `=1.0.0`). A `.mq.md`
file still renders normally in any compatible Markdown reader; rendering never
executes anything.

## What works (implemented and tested)

* **P0 contracts** — frozen Markdown profile, AST → program mapping, strict
  expression grammar, evaluation semantics; 150 AST goldens and 48 negative
  compile-error fixtures (`tests/`).
* **P1 interpreter** — native Rust tree-walk interpreter: values, scopes,
  functions, static parameter inference, assignments, branches, loops,
  returns, recursion, structured errors, fuel/stack/output budgets.
* **P2 references + CLI** — explicit YAML front-matter imports, `ModuleId`/
  `UnitId`/anchors, import cycle detection, multi-module calls,
  `run/call/check/inspect/fmt`, reference validation (`check --refs`) and the
  static link graph (`inspect --refs`).
* **P3 project** — `marqdo.yaml` validation, deterministic `marqdo.lock` for
  path dependencies, `init`, `add`/`remove` (manifest editing), `lock`,
  `sync --locked`.
* **P4 security core** — capability-checked host operations, project-relative
  FS globs with traversal rejection, budgets, effect trace, `pure`/`project`
  modes, and **`strict` mode refuses to run** because no OS isolation adapter
  exists.
* **P5 Math/Foreign** — native `Formula` value from `$$...$$`; `math.parse/
  eval/substitute` over a documented exact-rational subset; one-shot Python/
  Node kernels via `foreign.run/call` with typed wire protocol, timeout and
  output limits.
* **P6 plugin ABI** — frozen C ABI v1 header (`include/marqdo_plugin_abi_v1.h`),
  host bindings, Rust SDK (`mq_plugin_entry!`), `plugin.yaml` validation,
  target-triple + sha256 artifact verification, C demo, and explicitly trusted
  in-process plugin calls (`--plugin --trust-native-plugin --allow-plugin`).

Run everything:

```bash
cargo test --workspace
cargo test -p marqdo-cli        # conformance corpus + CLI contract tests
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Deliberate limits (do not assume)

* **Strict sandbox is not implemented.** `--mode strict` exits 1; there is no
  OS isolation or isolated plugin worker. `project` mode is cooperative and
  in-process.
* **Native plugins run in-process** and require an explicit unsafe trust flag.
  A plugin can crash or read the host; a C ABI is not a sandbox.
* **Package management is path-only in practice.** Git dependencies can be
  declared but cannot be resolved/locked; there is no registry, transitive
  resolution, `--offline`, `tree`, `update` or environment materialization.
  Manifest `capabilities` are requests, never grants.
* **Reference validation is opt-in**: `check --refs` reports broken links;
  plain `check` ignores them because links are static knowledge references.
  Link-form calls cannot cross modules; cross-module calls use imported alias
  expressions such as `geometry.圆面积(r=2)`.
* **`ref("#anchor")` is module-local** and errors with `ref.ambiguous` when a
  heading range contains more than one formula/code block instead of picking
  the first.
* **`fmt` is minimal** (line endings/trailing newlines), not AST-preserving
  reformatting.
* **Math is a limited calculator**, not a CAS/prover: no `simplify`/
  `differentiate`/matrices/series; equation solving is affine-only.
* **Foreign kernels are unsandboxed one-shot child processes** (no sessions,
  no state bridge); they run with the host user's filesystem/network access.
* Finite floats are IEEE-754 binary64; the typed wire round-trip is exact
  (`serde_json/float_roundtrip`), tagged integers never lose digits, and
  display formatting is decimal rather than bit-identical.

## Repository layout

```text
crates/marqdo-span            shared byte spans/source maps (leaf)
crates/marqdo-value           Value model, diagnostics, typed wire encoding
crates/marqdo-expression      strict expression/statement parser
crates/marqdo-markdown        frozen comrak profile, canonical AST, front matter
crates/marqdo-ir              module/unit/function/statement IR, builtin table
crates/marqdo-frontend        heading scopes, mapping, inference, static calls
crates/marqdo-math            exact-rational Formula subset
crates/marqdo-foreign         Python/Node one-shot kernel supervisor
crates/marqdo-sandbox         mode parsing + platform isolation detection
crates/marqdo-plugin-abi      C ABI v1 mirrors, manifest, artifacts, loader
crates/marqdo-plugin-sdk      Rust plugin SDK and entry macro
crates/marqdo-host            CapabilityHost, budgets, FS, math/foreign/plugin adapters
crates/marqdo-runtime         tree-walk interpreter
crates/marqdo-package         marqdo.yaml / marqdo.lock / root discovery
crates/cli                    CLI composition root (binary `marqdo`)
include/                      frozen plugin C ABI header
spec/                         normative language/format/ABI specs
docs/                         blueprint, ADRs, status, contributor guide
examples/                     runnable documents + multi-module + plugin demo
tests/                        conformance/negative corpus (see tests/README.md)
```

## Documentation index

* [`AGENTS.md`](AGENTS.md) — operating rules for contributors and coding agents.
* [`docs/DEVELOPMENT_BLUEPRINT.md`](docs/DEVELOPMENT_BLUEPRINT.md) — architecture
  baseline (Chinese; the M0.6 plan below supersedes it on command-surface scope,
  extension registration, plugin addressing, package identity, repository
  decoupling and release gates).
* [`docs/NEXT_PHASE_IMPLEMENTATION.md`](docs/NEXT_PHASE_IMPLEMENTATION.md) —
  proposed M0.6 plan (Chinese): extensibility fixes, callable/package/kernel
  registration, `marqdo tool`, Analysis/Trace v1, safety and release gates.
* [`docs/EXTENSIBILITY_AUDIT.md`](docs/EXTENSIBILITY_AUDIT.md) — source-backed
  audit issues E01–E16 with code locations and acceptance tests (Chinese).
* [`docs/contributor-guide.md`](docs/contributor-guide.md) — repo map, workflow,
  golden updates, quality gate.
* [`docs/STATUS.md`](docs/STATUS.md) — P0–P7 implementation matrix and honest
  open items; blueprint §18 Alpha checklist.
* [`docs/adr/`](docs/adr/) — accepted decisions (crate graph, inert executables,
  `ref` ambiguity, strict refusal, AST normalization, CLI contract, BigInt/wire,
  package identity, callable resolution, plugin export routing, capability
  broker, kernel providers, generic CLI tools, plugin-handle authority).
* Specs: [`markdown-profile.md`](spec/markdown-profile.md),
  [`mapping.md`](spec/mapping.md), [`expression.ebnf`](spec/expression.ebnf),
  [`evaluation.md`](spec/evaluation.md), [`references.md`](spec/references.md),
  [`value-wire.md`](spec/value-wire.md), [`security.md`](spec/security.md),
  [`package-format.md`](spec/package-format.md),
  [`plugin-abi.md`](spec/plugin-abi.md), [`math.md`](spec/math.md),
  [`foreign.md`](spec/foreign.md), [`tool-v1.md`](spec/tool-v1.md).
* [`tests/README.md`](tests/README.md) — conformance corpus and `UPDATE_GOLDENS`
  workflow.

## Examples

```bash
marqdo run examples/hello.mq.md
marqdo call examples/factorial.mq.md 数学.阶乘 --arg n=5
marqdo run examples/multi-module/index.mq.md
marqdo run examples/python-foreign.mq.md --allow-foreign   # needs python3 or node
marqdo inspect examples/references.mq.md --refs
marqdo inspect examples/hello.mq.md --platform             # strict: unavailable

./examples/plugin-demo/build.sh
marqdo plugin inspect examples/plugin-demo
marqdo run examples/plugin-demo/plugin-call.mq.md \
  --plugin examples/plugin-demo --trust-native-plugin --allow-plugin demo.solver
```

## References

* [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) ·
  [GFM](https://github.github.io/gfm/) · [Comrak](https://comrak.ee/)
* [Cargo](https://doc.rust-lang.org/cargo/) · [uv](https://docs.astral.sh/uv/)
* [Wasmtime Component Model](https://component-model.bytecodealliance.org/)
* Legacy Marqdo source (reference only): <https://github.com/cflmy/marqdo>
