# Marqdo Implementation Status (M0.5)

Snapshot: **2026-11-20**, workspace version `0.1.0`, plugin ABI `1.0`,
`wire_version` 1, Markdown profile `marqdo.md.v1`.

This file maps the blueprint's phases (`docs/DEVELOPMENT_BLUEPRINT.md` §13) to
concrete evidence in the repository and lists honest open items. It is a status
report, not a promise. "Done" means implemented **for the documented subset**
and covered by tests; it does not mean the blueprint's full ambition is
complete.

> **M0.6 planning cross-reference (2026-11-20)**: this file is the M0.5 evidence
> snapshot that the M0.6 audit was based on. Read
> [`docs/EXTENSIBILITY_AUDIT.md`](EXTENSIBILITY_AUDIT.md) (issues E01–E16) and
> the proposed [`docs/NEXT_PHASE_IMPLEMENTATION.md`](NEXT_PHASE_IMPLEMENTATION.md)
> alongside it. The plan does not claim any gap below is fixed. Per audit item
> E14, where this snapshot conflicts with the current tree (for example the CI
> workflow, console capability or `check --refs`), verify the source/tests at
> the current HEAD and update this table rather than trusting stale prose.

### R0 baseline record

At the M0.6 planning baseline (`0116160399fda6df69a72da438e04b0072566b0f`,
2026-11-20), the local gate passed before any M0.6 code change:

```text
cargo fmt --all -- --check                          ok
cargo clippy --workspace --all-targets -- -D warnings  ok
cargo test --workspace                              ok (150 AST + 36 semantic
                                                    + 48 compile-error fixtures)
```

CI evidence: `.github/workflows/ci.yml` runs the same Linux x86_64 gate plus CLI
example smoke tests. The R0 ADRs are `docs/adr/0008`–`0012` (package identity,
callable resolution, plugin export routing, capability broker, kernel
providers).

### M0.6 implementation progress

| Step | State | Evidence |
|---|---|---|
| R0 baseline + ADRs | **Done** | this section; `docs/adr/0008`–`0012`; full workspace gate |
| R1 package-aware identity + callable registry | **Done for statics** | `ModuleId { instance, relative_path }`; `PackageResolver` path sources; `lock_version: 2` with v1 migration; `CallableRegistry` for builtins/marqdo/declared ABI exports; external manifest wiring is R2 |
| R2 ABI/Kernel/Broker | **In progress** | `KernelRegistry` external providers; complete-name plugin export routing; `CapabilityBroker` with vendor policy; static `check --plugin` signatures; multi-export C test; provider tests. Isolated worker and handle table still pending |
| R3 Tool Protocol | **Done** | `tool install/list/remove`; unknown-command fallback; atomic local install; SHA/tamper checks; black-box tests for arbitrary names, Unicode args, conflicts, traversal and PATH hijacking |
| R4 Analysis/Trace v1 | **Done** | `check --emit` Analysis v1; `run/call --trace-file` Trace v1; deterministic schema tests; independent Python consumer; stale-artifact removal; value redaction |
| R5 environment/isolation | **Partial** | Pinned Git fetch/cache, transitive `@alias` resolution, atomic `.marqdo/env` materialization, foreign process-group cleanup. Strict OS isolation remains explicitly unavailable (fail-closed) |
| R6 release/consumers | **Done for Linux** | examples for unknown-name Tool, multi-export plugin, external kernel and multi-version packages; Analysis/Trace/Tool JSON Schemas; CI Linux gate + Windows/macOS `cargo check`; R6 example tests |

Reproduce the evidence:

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test --workspace
cargo test -p marqdo-cli
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

## 1. Phase matrix

| Phase | Status | Main evidence |
|---|---|---|
| **P0** Formal specs + parser goldens | **Done** | `spec/markdown-profile.md`, `spec/mapping.md`, `spec/expression.ebnf`, `spec/evaluation.md`; frozen profile test `crates/marqdo-markdown/src/profile.rs`; 150 AST fixtures + goldens in `tests/conformance/ast/`; 48 compile-error fixtures in `tests/negative/`; `inspect --ast`; `conformance_ast.rs`/`negative.rs` harnesses. |
| **P1** Interpreter, expressions, control flow | **Done** | `marqdo-expression`, `marqdo-ir`, `marqdo-frontend`, `marqdo-runtime`; factorial/countdown examples run without Python (`examples/factorial.mq.md`, `examples/hello.mq.md`); 36 execution goldens in `tests/conformance/semantics/`; runtime tests `crates/marqdo-runtime/tests/interpreter.rs`; prose invariance `crates/cli/tests/properties.rs`. |
| **P2** Modules, references, CLI experience | **Partial** | Imports/ModuleId/UnitId/anchor/cycle detection + `run/call/check/inspect/fmt` (`crates/cli`), multi-module example and `multi_module_imports_and_cycles` CLI test; `check --refs` reference validation (all loaded documents, on-demand static analysis of link targets) and `inspect --refs` static graph. Missing: cross-module link calls, rename/interface-lock tooling, canonical AST-preserving `fmt` (see §3). |
| **P3** Package environment | **Partial** | `marqdo.yaml`/`marqdo.lock` types+validation, `init`, `add`/`remove`, deterministic `lock`, `sync --locked`; M0.6 adds pinned Git fetch/cache, transitive `@alias` resolution and atomic `.marqdo/env` materialization. Missing: registry, `tree/update/vendor`, `--offline`, full conflict solver. |
| **P4** Security core and isolation | **Partial** | `CapabilityHost`, capabilities at execution points (console and math included), project-rooted FS with traversal rejection, budgets, effect trace, pure/project modes, **strict refuses without OS isolation** (`marqdo-sandbox`); CLI test `strict_mode_refuses_without_isolation`; host unit tests. Missing: OS isolation adapter, plugin worker, network enforcement. |
| **P5** Math and Foreign | **Partial (documented subset)** | `marqdo-math` exact-rational formula kernel for the grammar in `spec/math.md`; `math.parse/eval/substitute` host ops; `marqdo-foreign` Python/Node one-shot kernels with typed `foreign.call`; tests `crates/marqdo-math/tests/kernel.rs`, `crates/marqdo-foreign/tests/kernels.rs`; examples `math.mq.md`, `python-foreign.mq.md`. Missing: `simplify`/`differentiate`, matrices/series/symbolic algebra, foreign sessions, Python environment management. |
| **P6** Plugin ABI and developer experience | **Partial** | Frozen `include/marqdo_plugin_abi_v1.h`; C ABI mirrors/layout assertions, version negotiation, `plugin.yaml` validation, target triple + sha256 verification, wire envelopes, native loader (`marqdo-plugin-abi`); Rust SDK macro (`marqdo-plugin-sdk`); C demo (`examples/plugin-demo`); CLI `plugin inspect` and explicitly trusted in-process invocation (`--plugin`, `--trust-native-plugin`, `--allow-plugin`); tests `plugin_inspect_and_explicit_trusted_invocation`, `crates/marqdo-plugin-abi/tests/loader_unix.rs`. Missing: isolated worker transport, plugin capability bridge, installation, Rust plugin example crate. |
| **P7** Performance, release, CI | **Partial** | `.github/workflows/ci.yml` runs fmt/clippy/workspace tests plus CLI example smoke tests (including plugin inspect and invocation) on Linux x86_64. Missing: benchmarks, bytecode backend, cross-platform matrix, signed releases and supply-chain checks. Tree-walk interpreter only. |

## 2. Feature evidence (selected commands)

| Command | Verified behavior |
|---|---|
| `marqdo init demo` | creates `marqdo.yaml` (`>=0.1,<1`), `index.mq.md`, `.gitignore` (`.marqdo/`); refuses existing manifest (exit 1). |
| `marqdo check index.mq.md` | static only; 0 when no error diagnostics (warnings allowed); 1 on errors; JSON `command/success/errors/diagnostics`. |
| `marqdo run index.mq.md` | executes `main` through the Rust interpreter/host; program stdout on stdout; exit 1 on runtime/compile/strict errors. |
| `marqdo call file fn --arg n=5` | typed constant arguments; prints the non-`Unit` return value. |
| `marqdo inspect file --ast/--units/--signature/--ir/--refs/--platform --json` | static dashboards; `--platform` reports strict unavailable. |
| `marqdo fmt [--check]` | normalizes CRLF and trailing newlines only; exit 1 if `--check` detects a change. |
| `marqdo plugin inspect dir` | validates manifest + verifies sha256; never `dlopen`s (`loaded: false`). |
| `marqdo run doc --plugin dir --trust-native-plugin --allow-plugin demo.solver` | calls `solver.add(a=20,b=22)` and prints `42`; without trust/grant it exits 1. |
| `marqdo lock` / `sync --locked` | deterministic path-dependency lock; `--locked` fails on manifest/dependency drift. |

Test corpus at this snapshot (`tests/README.md`):

* 150 AST fixtures + goldens (`tests/conformance/ast/`);
* 36 semantic execution fixtures + goldens (`tests/conformance/semantics/`);
* 48 compile-time hard-error fixtures, 6 inert-executable warning fixtures,
  10 runtime-error fixtures (`tests/negative/`);
* `crates/cli/tests/` harnesses: `conformance_ast`, `conformance_semantics`,
  `negative`, `properties`, `integration_cli`;
* crate tests across all 15 crates, including real `python3`/`node` foreign
  tests (skipped if unavailable) and a compiled C plugin test (skipped without
  `cc`).

At this snapshot `cargo test -p marqdo-cli` passes all 21 harness tests
(2 CLI unit + AST/semantics/negative/properties/integration) and
`cargo test --workspace` passes. The fixture counts above match
`tests/README.md`; empty `tests/integration`, `tests/properties`,
`tests/security` and `tests/abi` directories are reserved for future
workstreams — the active corpus lives in `tests/conformance`, `tests/negative`
and `crates/cli/tests`.

## 3. Open items by phase

### P0

* Wire floats now round-trip exactly: `serde_json`'s `float_roundtrip`
  feature is enabled and a Python `math.pi * 9` fixture pins the exact value
  (`spec/value-wire.md` §3).
* Profile change safety relies on the frozen-options unit test; no CI gate
  regenerates goldens (intentional).

### P1

* The blueprint asks for a draft Turing-completeness argument. The language
  supports mutable locals, branch-first-match, per-item `while` loops and
  recursion with bounded budgets; a formal encoding write-up is still missing.
* The default cooperative stack budget is 128 frames so very deep recursion
  yields a structured `resource.exhausted` instead of exhausting the debug-build
  native stack; `--stack` raises it deliberately. A true guard-page/iterative
  interpreter hardening is still open.

### P2

* `fmt` is intentionally minimal (line endings/trailing newlines only), not the
  blueprint's "canonical AST-equivalent formatting"; no semantic formatting,
  AST-equivalence check or render-preserving rewrite exists.
* Reference validation is opt-in: `check --refs` (and `inspect --refs`) resolve
  links; plain `check` does not fail on broken links because links are static
  knowledge references rather than execution inputs.
* Link-form calls cannot cross modules; cross-module calls use imported alias
  expressions. The blueprint's `> [f](./other.mq.md#f)(...)` example is not
  supported.
* `ref("#...")` is module-local and does not accept a selector for units with
  multiple resources (it errors with `ref.ambiguous`, ADR-0003).
* No rename tooling, interface lock/ABI diffing, or incremental cache.
* Duplicate/nested project boundaries are not diagnosed (nearest
  `marqdo.yaml` wins).

### P3

* Git dependencies can be declared (`git` + `rev`, validated) but cannot be
  resolved or locked: `lock` errors "git dependencies are not supported by this
  build".
* No transitive dependency graph, conflict resolution, registry, publishing,
  signing, `tree`, `update`, `vendor`, `--offline`, or `lock --locked`.
* No environment materialization (`.marqdo/env`, caches, kernels, sessions);
  `sync` verifies but never installs or rewrites.
* `add`/`remove` are shallow manifest editors; adding a git dependency succeeds
  at the manifest level even though `lock` will later refuse it.
* JSON errors from `add`/`remove`/`lock`/`sync` are plain stderr even with
  `--json`.

### P4

* **Strict mode cannot run at all**: no OS isolation adapter (Linux
  namespaces/seccomp/cgroups, Windows Job Object/appcontainer, macOS sandbox)
  and `worker_processes: false`; `run --mode strict` exits 1 (ADR-0004).
* `console` is a capability checked in `op_print`; it is granted by default in
  `pure`/`project` mode and is not a bypass of the capability boundary.
* Network capability (`net_hosts`, `check_net`) exists but no host operation
  performs network I/O and there is no `--allow-net` flag.
* Write-path symlink TOCTOU caveat (`spec/security.md` §2.1); symlink/race
  hardening open.
* Budgets do not cover every allocation path; the interpreter's containers are
  bounded only by fuel and available memory.
* Effect trace does not record argument values or paths (privacy-positive, but
  less diagnostic detail); trace is opt-in and implicitly on for `--json`.
* `plugin_handle` wire values are structurally reconstructable, not
  unforgeable capability tokens.

### P5

* Math is a deliberately limited calculator subset: no `simplify`,
  `differentiate`, matrices, sums/integrals, trig/log functions, units,
  assumptions or theorem proving. `math.simplify`/`math.differentiate` are
  host ops that always return `math.unsupported`.
* Equation solving is affine-only, single target, target must occur exactly
  once; other shapes are `math.unsupported`.
* Division by zero inside a formula maps to `runtime.type` (via
  `MathErrorKind::Type`), not `runtime.division_by_zero`; documented divergence.
* Foreign kernels are one-shot only: no sessions, state bridge, Python/Node
  environment management, or network policy for child processes.
* Foreign child processes are terminated only when the direct child is killed;
  grandchildren are not tracked.
* Foreign per-call timeout (10 s) and output cap (1 MiB) are not CLI-configurable.

### P6

* No isolated plugin worker (length-prefixed IPC) or Wasm transport; only
  explicitly trusted in-process native loading. A native plugin crash is a host
  crash (`spec/plugin-abi.md` §1/§9).
* No plugin capability bridge from the CLI: plugins are loaded with no
  capability handler, so every plugin `request_capability` is denied.
* Plugin calls are not statically validated; unknown namespaces are runtime
  `symbol.unknown`. Positional plugin arguments are rejected.
* One namespace per plugin (first export prefix); multi-prefix manifests are not
  reachable beyond the first prefix.
* No plugin installation/package management; `plugins:` in `marqdo.yaml` is
  declarative only. No Rust plugin example crate (SDK has doc examples/tests).
* No hot unload/reload (ABI v1 design).

### P7

* Performance work is untouched: no benchmark suite, no bytecode backend or
  differential tests against the tree-walker, no parse/compile caching.
* CI exists (`.github/workflows/ci.yml`) and runs fmt/clippy/workspace tests
  plus CLI smoke tests on Linux x86_64; missing: Windows/macOS matrix,
  sanitizers/fuzz gates, signing, release automation or supply-chain scanning.
* No published version, no compatibility matrix beyond the frozen ABI/format
  documents.

## 4. Blueprint §18 "Alpha Definition of Done" mapping

| §18 item | Status | Evidence / gap |
|---|---|---|
| Run/call non-trivial recursive, branch, loop, call, parameter-inference programs without Python | **Met** | factorial/recursion (`examples/factorial.mq.md`, runtime + semantic goldens), countdown loop, multi-module alias call; interpreter is native Rust. |
| `.mq.md` readable under the chosen CommonMark/GFM/Math profile; standard AST behavior covered by goldens | **Met** | frozen `marqdo.md.v1`, 150 AST goldens, profile option test, `spec/markdown-profile.md`. |
| No heuristic downgrade of execution semantics; accurate source locations for invalid execution nodes | **Mostly met** | hard errors with spans for function-body executable shapes and negative corpus. Gaps: inert executable outside functions is a warning by design (ADR-0002); cross-module runtime error spans resolve against the entry file only; some lazy nested nodes are inert without a warning. |
| `check`/`inspect` side-effect free; imports, calls and static references verifiable across files | **Mostly met** | static load/analysis only; imports + cycle detection; cross-module calls validated; `check --refs` validates references across loaded documents and statically analyses link targets; `inspect --refs` prints the entry graph. |
| Minimal manifest, lock file, path/Git dependencies and environment sync | **Partial** | manifest/lock/path works, `sync --locked` verifies; git resolution, transitive deps, environment materialization and `--offline` missing. |
| All host side effects capability-authorized; untrusted code/native plugins cannot masquerade as strict sandbox | **Mostly met** | capability checks at execution points; strict refuses; native plugins require explicit trust and are documented as unsandboxed. Gaps: network enforcement not implemented (no network builtin is exposed), no worker isolation. |
| Formula computational subset explicit/testable; Foreign I/O, timeout and permissions explicit | **Met for the documented subset** | `spec/math.md`, `spec/foreign.md`; math kernel tests; foreign tests (timeout, protocol, output cap, env); permission gating tests/smoke. Simplification/differentiation explicitly unsupported. |
| Plugin C ABI v1, Rust SDK, C example, isolated worker basic path, ABI compat error tests | **Partial** | ABI/SDK/C example/loader/ABI-error tests exist; **isolated worker path missing**. |
| CI has at least one full research pipeline: Markdown function → Math → Foreign → Plugin → controlled output | **Partial** | CI now runs fmt/clippy/tests and CLI smoke tests; individual math/foreign/plugin paths are covered by examples and tests, but no single end-to-end pipeline document/test exists yet. |
| Examples, README, AGENTS and contributor docs allow a first change without legacy context | **Met** | `README.md`, `AGENTS.md`, this guide, `docs/STATUS.md`, specs, ADRs, `tests/README.md`, examples. |

**Conclusion: the Alpha definition of done is not fully met.** P0/P1 and the
documented P5 subset are solid; P2/P3/P4/P6 are functional but partial; P7 is
absent. The blocking items for an Alpha claim are the strict sandbox/worker
isolation story (P4/P6), git/environment package management (P3), and a CI
end-to-end pipeline (P7).

## 5. What is deliberately limited (do not overclaim)

* Only a single, frozen Markdown profile (`marqdo.md.v1`); changing parser
  options/versions is a language change.
* Strict mode is a refusal, not an isolation implementation. `project` mode is
  cooperative and in-process.
* Native plugins are trusted in-process code; there is no isolated plugin
  worker, and a plugin crash takes down the host.
* Ints are arbitrary precision; finite floats are binary64 and wire round-trip
  is exact (`float_roundtrip` enabled), but decimal-vs-binary formatting still
  applies when displaying values.
* Math is a limited exact-rational calculator, not a CAS or a prover.
* Foreign kernels are optional, one-shot, unsandboxed child processes.
* `fmt` is minimal; plain `check` does not validate Markdown links (only
  `check --refs` does); link-form calls cannot cross modules.
* Package management is path-only in practice; `marqdo.yaml` capabilities are
  requests, never grants.
