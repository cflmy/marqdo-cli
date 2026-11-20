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

> **Status: stabilization candidate, not released.** The language, lock v3,
> Analysis v2, Trace v2, Tool v1, Plugin ABI v1 (native + worker) and pinned
> Foreign providers are implemented and covered by tests on Linux. The
> macOS/Windows `E0433` platform-cfg failure and the Linux strict
> probe→launch race are fixed in source at `8af7262`, but the native
> Windows/macOS and GitHub Linux lanes have not been re-run from this
> checkout. **`strict` OS isolation is available on Linux only when the
> runtime Bubblewrap probe passes**; it was verified against the full
> escape/resource matrix in a disposable Ubuntu 26.04 VM at `8af7262`
> (ADR-0021). macOS/Windows fail closed, strict L2 (Worker/Foreign) is not
> implemented, and the broker/worker are not a sandbox. The release decision
> is **DO NOT RELEASE** until the remote gates have same-SHA URLs. See
> [`docs/STATUS.md`](docs/STATUS.md), [`COMPATIBILITY.md`](COMPATIBILITY.md)
> and [`docs/security-gate.md`](docs/security-gate.md). The single current CI
> convergence plan is
> [`docs/NEXT_RELEASE_CI_CONVERGENCE.md`](docs/NEXT_RELEASE_CI_CONVERGENCE.md).

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
  expression grammar, evaluation semantics; 160 AST goldens, 71 semantic
  goldens, 52 compile-error fixtures and 21 runtime-error fixtures
  (`tests/`).
* **P1 interpreter** — native Rust tree-walk interpreter: values, scopes,
  functions, static parameter inference, assignments, branches, loops,
  returns, recursion, structured errors, fuel/stack/output budgets.
* **Addressable memory (RFC-CM P1)** — opaque run-scoped `Memory` regions with
  64 KiB zero-filled pages, grow-only `memory.new/size/grow`, bounded byte
  read/write, `m[i] = byte`, run/per-region budgets, host-boundary and wire
  rejection (ADR-0022).
* **Minimal structure primitives (RFC-CM CM-A04)** — value-semantics
  `list.append/concat`, `map.keys/has/get/set/remove`, Unicode scalar
  `text.codepoints/from_codepoint`, and non-negative `table.columns/row/cell`;
  pure builtins with no mutation or new syntax (ADR-0023).
* **Recoverable results (RFC-CM CM-A05)** — `{ok:true,value}` /
  `{ok:false,code,message}` as an ordinary Map, with a `.mq.md` first-fit
  free-list allocator demonstrating reuse, splitting and `alloc.no_space`
  recovery (ADR-0024). No `Result` type or exception syntax.
* **Bundled stdlib + print prototype (RFC-CM CM-A07/A10)** — offline
  `stdlib/` with explicit `std:io`/`std:format`/`std:text` imports, SHA-256
  digest verification, Analysis identity and a frozen-CLI closure test; the
  `.mq.md` print chain is the only execution path for `打印`/`print` on an official install (ADR-0031),
  covering all 15 value types; `HostOp::Print` and the legacy formatter are
  removed (ADR-0027/0028/0029/0030/0031).
* **Blocking TCP endpoint (RFC-CM P5 first slice)** — `resource.open("tcp",
  "<ip>:<port>", ["read","write"])` with exact IP grants, project mode and
  5-second timeouts; DNS/TLS/UDP/server sockets excluded (ADR-0034).
* **In-process ResourceProvider v1 (RFC-CM CM-A09 first slice)** — trusted
  providers register extra resource kinds, project-mode + name-grant gated,
  using the existing `resource.*`/`Result` ABI; no parser/runtime/`HostOp`
  change (ADR-0033).
* **Table stdlib algorithms (RFC-CM CM-A08)** — `stdlib/data/table.mq.md`
  select/filter/sort/append/set/join/group over the logical `Table` value;
  value-returning, input-immutable, with GFM AST laziness unchanged (ADR-0032).
* **Resource streams (RFC-CM CM-A06)** — opaque host-issued `Resource` handles;
  `resource.open/read/write/close` transfer bytes through `Memory`; stdout
  write in pure/project, granted project-file read, and buffered project-file
  write flushed on `close` in project; strict refuses all resource opens until
  verified (ADR-0025/0026).
* **P2 references + CLI** — explicit YAML front-matter imports, `ModuleId`/
  `UnitId`/anchors, import cycle detection, multi-module calls,
  `run/call/check/inspect/fmt`, reference validation (`check --refs`) and the
  static link graph (`inspect --refs`).
* **P3 project** — lock v3 full transitive package-instance graph for path and
  pinned Git sources, verified content-addressed Git cache, deterministic
  `lock`, strict `sync --locked` with atomic `.marqdo/env` materialization.
  `run`/`call`/`check` never fetch or rewrite the lock.
* **P4 security core** — capability-checked host operations, project-relative
  FS globs with traversal rejection, budgets, effect trace, `pure`/`project`
  modes, and **Linux `strict` mode** using Bubblewrap namespaces, no-new-privs,
  capability dropping, rlimits, a wall-clock supervisor and child
  self-verification (ADR-0021). It is probe-gated and verified in a disposable
  Ubuntu VM; unsupported hosts fail closed.
* **P5 Math/Foreign** — native `Formula` value from `$$...$$`; `math.parse/
  eval/substitute` over a documented exact-rational subset; one-shot Python/
  Node kernels via `foreign.run/call` with typed wire protocol, timeout and
  output limits; pinned provider/environment digests and ADR-0020 environment
  binding.
* **P6 plugin ABI** — frozen C ABI v1 header (`include/marqdo_plugin_abi_v1.h`),
  host bindings, Rust SDK (`mq_plugin_entry!`), `plugin.yaml` validation,
  target-triple + sha256 artifact verification, native and isolated worker
  transports, brokered `fs.read`, and explicitly trusted in-process calls
  (`--plugin --trust-native-plugin --allow-plugin`). A C ABI is not a sandbox.
* **Public protocols** — Analysis v2 and Trace v2 with JSON Schemas and
  independent Python consumers; Tool Protocol v1 with arbitrary command names,
  SemVer negotiation and whole-bundle digest verification.

Run everything:

```bash
cargo test --workspace
cargo test -p marqdo-cli        # conformance corpus + CLI contract tests
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Deliberate limits (do not assume)

* **Strict is Linux-only and L1-only.** `--mode strict` enables the
  Bubblewrap supervisor only when its runtime probe passes; otherwise it exits
  1. It supports pure Marqdo + Math and rejects Foreign, plugins, kernels,
  external grants and trace files (exit 2). Worker plugins provide
  crash/address-space isolation, not an OS sandbox, and are not allowed in
  strict.
* **Native plugins run in-process** and require an explicit unsafe trust flag.
  A plugin can crash or read the host; a C ABI is not a sandbox.
* **Package management is path + pinned-Git only.** There is no registry,
  floating-branch resolution, conflict solver, `--offline`, `tree`, `update`,
  `vendor`, or automatic uv/npm environment creation. Manifest
  `capabilities` are requests, never grants.
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
* [`docs/STABLE_RELEASE_IMPLEMENTATION.md`](docs/STABLE_RELEASE_IMPLEMENTATION.md)
  — **the single authoritative next-phase plan**: PR order, G-A/G-B/G-C gates,
  Windows/macOS fixes, protocol/environment stabilization, strict isolation
  requirements and the release checklist.
* [`COMPATIBILITY.md`](COMPATIBILITY.md) — protocol versions, per-platform
  feature evidence and explicit unsupported features.
* [`CHANGELOG.md`](CHANGELOG.md) — unreleased stabilization changes and the
  `0.1.0` baseline.
* [`docs/security-gate.md`](docs/security-gate.md) — G-C evidence and limits;
  [`docs/security-vm.md`](docs/security-vm.md) reproduces the disposable VM and
  the self-hosted security runner.
* [`docs/releasing.md`](docs/releasing.md) — artifact, checksum, clean-install
  smoke, tagging and rollback procedure.
* [`docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`](docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md)
  — historical stabilization spec (baseline `0adebe4`), superseded on current
  priorities by the stable-release plan.
* [`docs/NEXT_PHASE_IMPLEMENTATION.md`](docs/NEXT_PHASE_IMPLEMENTATION.md) —
  historical M0.6 plan.
* [`docs/EXTENSIBILITY_AUDIT.md`](docs/EXTENSIBILITY_AUDIT.md) — source-backed
  audit issues E01–E16.
* [`docs/contributor-guide.md`](docs/contributor-guide.md) — repo map, workflow,
  golden updates, quality gate.
* [`docs/STATUS.md`](docs/STATUS.md) — current work-package status, local
  evidence and open blockers.
* [`docs/adr/`](docs/adr/) — accepted decisions (crate graph, inert executables,
  `ref` ambiguity, strict refusal, AST normalization, CLI contract, BigInt/wire,
  package identity, callable resolution, plugin export routing, capability
  broker, kernel providers, generic CLI tools, plugin-handle authority, public Analysis/Trace v1).
* Specs: [`markdown-profile.md`](spec/markdown-profile.md),
  [`mapping.md`](spec/mapping.md), [`expression.ebnf`](spec/expression.ebnf),
  [`evaluation.md`](spec/evaluation.md), [`references.md`](spec/references.md),
  [`value-wire.md`](spec/value-wire.md), [`security.md`](spec/security.md),
  [`package-format.md`](spec/package-format.md),
  [`plugin-abi.md`](spec/plugin-abi.md), [`math.md`](spec/math.md),
  [`foreign.md`](spec/foreign.md), [`tool-v1.md`](spec/tool-v1.md),
  [`analysis-v2.md`](spec/analysis-v2.md), [`trace-v2.md`](spec/trace-v2.md);
  JSON Schemas in [`spec/schemas/`](spec/schemas/).
* [`tests/README.md`](tests/README.md) — conformance corpus and `UPDATE_GOLDENS`
  workflow.

## Examples

```bash
marqdo run examples/hello.mq.md
marqdo call examples/factorial.mq.md 数学.阶乘 --arg n=5
marqdo run examples/multi-module/index.mq.md
marqdo run examples/python-foreign.mq.md --allow-foreign   # needs python3 or node
marqdo run examples/packages-multi-version/index.mq.md
marqdo run examples/kernel-mock/demo.mq.md \
  --kernel examples/kernel-mock/kernel.json --allow-foreign
marqdo inspect examples/references.mq.md --refs
marqdo inspect examples/hello.mq.md --platform             # strict: unavailable

./examples/plugin-demo/build.sh
marqdo plugin inspect examples/plugin-demo/build
marqdo run examples/plugin-demo/plugin-call.mq.md \
  --plugin examples/plugin-demo/build --trust-native-plugin --allow-plugin demo.solver

./examples/plugin-multi-export/build.sh
marqdo run examples/plugin-multi-export/plugin-call.mq.md \
  --plugin examples/plugin-multi-export/build --trust-native-plugin --allow-plugin demo.solver

marqdo tool install examples/tool-unknown-name/tool.json
marqdo benchmark run .
marqdo tool remove benchmark
```

## References

* [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) ·
  [GFM](https://github.github.io/gfm/) · [Comrak](https://comrak.ee/)
* [Cargo](https://doc.rust-lang.org/cargo/) · [uv](https://docs.astral.sh/uv/)
* [Wasmtime Component Model](https://component-model.bytecodealliance.org/)
* Legacy Marqdo source (reference only): <https://github.com/cflmy/marqdo>
