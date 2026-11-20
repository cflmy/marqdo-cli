# Marqdo Contributor Guide

This guide is for people changing the Marqdo CLI workspace. It assumes you have
read `AGENTS.md` (the non-negotiable architectural rules) and
`docs/DEVELOPMENT_BLUEPRINT.md` §0/§1/§11/§13/§14. Formal contracts live in
`spec/`; decisions live in `docs/adr/`.

## 1. Repository map

```text
marqdo-cli/
├── Cargo.toml                  workspace (15 crates, resolver 2)
├── AGENTS.md                   operator rules for contributors/agents
├── README.md                   user-facing quickstart and limits
├── docs/
│   ├── DEVELOPMENT_BLUEPRINT.md   architecture baseline
│   ├── STABLE_RELEASE_IMPLEMENTATION.md  authoritative next-phase/release plan
│   ├── STATUS.md                  current evidence, work packages, blockers
│   ├── security-gate.md           G-C probe evidence (strict stays closed)
│   ├── releasing.md               artifact/checksum/smoke/tag procedure
│   ├── CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md  historical stabilization spec
│   ├── NEXT_PHASE_IMPLEMENTATION.md  historical M0.6 plan
│   ├── EXTENSIBILITY_AUDIT.md     source-backed E01–E16 audit
│   ├── history/                   archived status snapshots
│   ├── contributor-guide.md       this file
│   └── adr/0001..0020-*.md        accepted decisions
├── spec/
│   ├── markdown-profile.md        frozen parser profile `marqdo.md.v1`
│   ├── mapping.md                 AST → program roles
│   ├── expression.ebnf            executable payload grammar
│   ├── evaluation.md              values, numbers, control flow, builtins
│   ├── references.md              imports/references/calls/ref()
│   ├── value-wire.md              typed wire_version 1 values
│   ├── security.md                modes, capabilities, budgets, trust
│   ├── package-format.md          marqdo.yaml / marqdo.lock / init
│   ├── plugin-abi.md              C ABI v1, SDK, host lifecycle
│   ├── math.md                    Formula subset and math.* semantics
│   ├── foreign.md                 Python/Node kernel protocol
│   ├── tool-v1.md                 generic external Tool Protocol v1
│   ├── analysis-v2.md             public Analysis v2 projection (v1 historical)
│   ├── trace-v2.md                public Trace v2 projection (v1 historical)
│   └── trace-v1.md                public Trace v1 event stream
├── crates/
│   ├── marqdo-span/               Span, LineCol, LineIndex (leaf)
│   ├── marqdo-value/              Value, diagnostics, errors, wire encoding
│   ├── marqdo-expression/         lexer/parser/AST for executable payloads
│   ├── marqdo-markdown/           comrak profile, canonical AST, front matter
│   ├── marqdo-ir/                 Module/Unit/Function/Stmt IR, builtin table
│   ├── marqdo-frontend/           units, map, signature, infer, calls, resources
│   ├── marqdo-math/               Formula parser/evaluator (exact rationals)
│   ├── marqdo-foreign/            Python/Node one-shot kernel supervisor
│   ├── marqdo-sandbox/            modes + platform capability detection
│   ├── marqdo-plugin-abi/         C ABI mirrors, manifest, artifact, loader, wire
│   ├── marqdo-plugin-sdk/         Rust plugin SDK and `mq_plugin_entry!`
│   ├── marqdo-host/               CapabilityHost, budgets, FS, math/foreign/plugin adapters
│   ├── marqdo-runtime/            tree-walk interpreter, value ops, builtins
│   ├── marqdo-package/            marqdo.yaml / marqdo.lock types + discovery
│   └── cli/                       composition root: flags, commands, loader, output
├── include/marqdo_plugin_abi_v1.h frozen C ABI v1 header
├── examples/                      hello, factorial, math, references, python-foreign,
│                                  multi-module/, plugin-demo/
├── tests/
│   ├── conformance/ast/           *.mq.md + *.ast.json AST goldens
│   ├── conformance/semantics/     *.mq.md + *.expected.json execution goldens
│   ├── negative/                  compile-time + runtime hard-error fixtures
│   ├── README.md                  corpus layout and golden workflow
│   ├── integration/ properties/ security/ abi/   reserved dirs (see STATUS)
└── crates/cli/tests/              Rust harnesses for the corpus and CLI
```

## 2. Dependency direction (must stay acyclic)

```text
span          -> serde
value         -> span
expression    -> span
markdown      -> span, comrak           (no semantic crate!)
ir            -> span, expression
frontend      -> markdown, expression, ir, value
host          -> value, math, foreign, sandbox
runtime       -> ir, value, host, expression
math          -> value
foreign       -> value
plugin-abi    -> value
plugin-sdk    -> plugin-abi
package       -> serde only
sandbox       -> serde only
cli           -> everything (composition root)
```

Rules (from blueprint §11.1, adjusted by ADR-0001):

* `markdown` owns the only Markdown parser; it must not depend on semantic
  crates, the host or the runtime. The single exception to "zero Marqdo deps"
  is `marqdo-span`, which is a dep-free leaf shared by all layers.
* `frontend`/`ir`/`runtime` never depend on `cli`.
* `runtime` never parses Markdown and never performs I/O directly: all effects
  go through `marqdo_host::Host`.
* `host` never depends on the CLI; kernels/plugins are adapted here.
* The CLI is the composition root: flag parsing, project loading, rendering and
  process exit. It must not become a second interpreter.
* Check the graph with `cargo tree` (or `cargo tree -i <crate>`); `runtime ->
  cli`, `markdown -> frontend`, and dependency cycles are review blockers.

## 3. Adding a feature or fixture (workflow)

The project uses a fixture-first workflow (blueprint §13/§14, AGENTS §
"Development workflow"). The expected sequence is:

1. **Find the rule.** Identify the affected spec section and blueprint section.
   If the change alters observable language semantics, package/lock format,
   ABI, authorization defaults or public CLI behavior, write/supersede an ADR
   first (`docs/adr/`, template in blueprint §16.2).
2. **Write a failing fixture.** Add a small `.mq.md` positive and/or negative
   case under the right directory:
   * AST shape: `tests/conformance/ast/NNN-name.mq.md` + golden
     `NNN-name.ast.json`;
   * semantics: `tests/conformance/semantics/NNN-name.mq.md` +
     `NNN-name.expected.json` (typed wire `args`/`value` + captured `stdout`);
   * hard error: `tests/negative/NNN-name.mq.md` + `{"codes":[...]}`;
   * inert-executable warning: `tests/negative/warnings/`;
   * runtime error: `tests/negative/runtime/` with `{"function","args","codes"}`
     (optional `grants`, `stack_limit`).
   Run the relevant harness (see §6) and confirm it fails for the expected
   reason.
3. **Update the spec.** `spec/*.md` is normative. If implementation and spec
   disagree, fix whichever is wrong and note the tradeoff. Do not silently
   invent behavior in code and leave the spec stale.
4. **Implement the smallest module.** Keep the layers separated: parser
   (`marqdo-markdown`), semantic mapping (`marqdo-frontend`), IR
   (`marqdo-ir`), interpreter (`marqdo-runtime`), effects (`marqdo-host`),
   composition (`cli`). Never add a second Markdown parser, a regex-based
   syntax decision, or a host bypass.
5. **Refresh goldens deliberately** (§4) and run the full gate (§6).
6. **Write the tradeoff/limitations** in `docs/STATUS.md` when a P-level claim
   changes, and in the PR description (paths, commands run, skipped tests).

If a fixture reveals an implementation bug, fix the code, not the expected
behavior. Negative fixtures must assert hard errors — never weaken them to
warnings or prose fallbacks.

## 4. Golden update workflow

AST goldens (`tests/conformance/ast/*.ast.json`) are the serialized canonical
AST from `marqdo_markdown::parse(source).root`. Regenerate **only** for an
intentional, ADR-backed profile change:

```bash
UPDATE_GOLDENS=1 cargo test -p marqdo-cli --test conformance_ast
```

The update run rewrites every golden, prints the count and passes. Re-run
without the variable and review the diff: **a golden change is a language
change, not a bug fix**. A fixture without a golden and a golden without a
fixture are both errors.

Semantic goldens (`*.expected.json`): `UPDATE_SEMANTICS_GOLDENS=1` exists for
authoring convenience, but never use it blindly — verify every value against the
fixture source first. Invalid, inconsistent or unverified expected values defeat
the corpus.

Negative goldens are hand-written expected code lists (exact sorted multiset for
compile-time errors; warning suites; runtime suites). Do not auto-generate them.

Adding/removing a fixture and its golden must happen in the same change.
Fixtures are data: tests must not commit, copy or rewrite files outside a
throw-away temp directory. `tests/README.md` is the in-repo reference for corpus
counts, harness responsibilities and known observations.

## 5. Coding conventions

* Rust edition 2021, `rust-version = 1.82`, workspace version `0.1.0`.
* `unsafe` is forbidden in pure crates (`marqdo-math`, `marqdo-expression` use
  `#![forbid(unsafe_code)]`); the ABI/loader crates contain the only
  `unsafe` code and every block has a SAFETY comment explaining the contract.
* No panics on untrusted input: Markdown, expressions, LaTeX, wire data,
  manifests and foreign output all produce structured errors. `unwrap()`/
  `expect()` are acceptable only for genuinely impossible invariants or in
  tests.
* Stable diagnostic/runtime **codes** are part of the contract; human messages
  are not. Add new codes to `crates/marqdo-value/src/diagnostic.rs` and map
  categories in `error.rs`.
* Values cross layers via `marqdo_value::Value` and the typed wire format
  (`spec/value-wire.md`); never serialize `Int` as a JSON number.
* `check`/`inspect` must remain side-effect free: no execution, no foreign
  process, no plugin `dlopen`, no file writes.
* Prose and executable nodes mix freely; prose never causes an action.
* Formatting: `rustfmt.toml` is authoritative; keep `cargo fmt --check` clean.

## 6. Build, test and quality gate

The workspace is offline-cached. Use the workspace cache as documented in
`tests/README.md` / AGENTS.md:

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
```

Commands:

```bash
cargo build -p marqdo-cli          # binary: target/debug/marqdo
cargo test --workspace             # all crate unit + integration tests
cargo test -p marqdo-cli           # conformance corpus + CLI integration tests
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Harness map (`crates/cli/tests/`):

| Harness | Proves |
|---|---|
| `conformance_ast.rs` | frozen profile parses fixtures into checked-in AST (kinds, attributes, literals, spans, order) |
| `conformance_semantics.rs` | frontend + IR + runtime + host produce expected wire value/stdout |
| `negative.rs` | invalid executable shapes are hard errors; warning and runtime suites |
| `properties.rs` | prose-invariance (pure narrative edits do not change executable IR modulo spans) |
| `integration_cli.rs` | real CLI contract: init/check/run/call/inspect/fmt, multi-module, lock/sync, plugin trust and invocation |

Crate-level tests worth knowing:

* `marqdo-expression` — grammar/priority/nesting/limit tests;
* `marqdo-math/tests/kernel.rs` — exact arithmetic, bounds, equations, unsupported constructs;
* `marqdo-foreign/tests/kernels.rs` — real python3/node (skips if absent), timeout, output caps, env, wire protocol;
* `marqdo-plugin-abi/tests/loader_unix.rs` — compiles the C demo with `cc`, loads it, ABI-mismatch/negative cases (skips without `cc`);
* `marqdo-runtime/tests/interpreter.rs` — recursion, loops, returns, budgets, ambiguity;
* `marqdo-host` unit tests — capabilities, project FS, budgets.

Tests that need Python/Node/`cc` print a skip notice and pass when unavailable;
record such skips in the PR.

## 7. Running examples

From the repository root (after `cargo build -p marqdo-cli`):

```bash
B=target/debug/marqdo
$B run examples/hello.mq.md
$B call examples/factorial.mq.md 数学.阶乘 --arg n=5      # prints 120
$B check examples/references.mq.md
$B inspect examples/references.mq.md --refs
$B inspect examples/hello.mq.md --ast --json
$B run examples/math.mq.md                                # has no main: use call
$B call examples/math.mq.md 数学.计算圆面积
$B run examples/python-foreign.mq.md --allow-foreign        # needs python3
$B run examples/multi-module/index.mq.md
(cd examples/multi-module && ../../$B lock && ../../$B sync --locked)

# Plugin demo (builds a host-specific manifest under build/)
./examples/plugin-demo/build.sh
$B plugin inspect examples/plugin-demo/build
$B run examples/plugin-demo/plugin-call.mq.md \
   --plugin examples/plugin-demo/build --trust-native-plugin --allow-plugin demo.solver
```

Notes: `run` defaults to `index.mq.md` and executes `main`; `call` requires a
file and function; root-only examples (for example `math.mq.md`) define no
`main`. `--mode strict` is expected to refuse to run (ADR-0004).

## 8. Docs, ADRs and commit policy

* `spec/` = normative contracts; `docs/` = architecture/decisions/status;
  `README.md` = user quickstart; `AGENTS.md` = contributor rules.
* `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md` is the current-stage
  stabilization spec (baseline `0adebe4`). It supersedes the M0.6 planning pair
  on the current baseline, gap list (B-01–B-18) and the G-A/G-B/G-C release
  gates. `docs/NEXT_PHASE_IMPLEMENTATION.md` and
  `docs/EXTENSIBILITY_AUDIT.md` remain the M0.6 planning/evidence pair. All are
  **Proposed**: the M0.6 plan supersedes the blueprint only on command-surface
  scope, extension registration, plugin addressing, package identity, repository
  decoupling and release gates.
  Language semantics remain normative in `spec/` + Accepted ADRs + tests, and
  `spec/tool-v1.md` is a candidate protocol until an ADR and implementation
  tests land. Never cite these proposed documents as evidence that a feature
  exists.
* Changes to language semantics, AST mapping, wire format, plugin ABI,
  package/lock format, authorization defaults or public CLI behavior require an
  **Accepted ADR** (or superseding one) plus spec updates and tests. Use the
  blueprint §16.2 template: Context / Decision / Alternatives / Markdown AST
  shape / Compatibility / Security / Testing / Migration.
* ADRs are immutable once Accepted: add a new ADR with
  `Status: Superseded by ADR-NNNN` rather than rewriting history (typo fixes
  aside).
* Docs must describe what the code does. When implementation and blueprint
  disagree, document the implementation and flag the divergence explicitly as
  a limitation/open item; never invent behavior.
* Do not edit generated/vendored state, and never modify tests to hide a
  regression.
* Commit in reviewable units with a message stating the affected spec/ADR and
  the commands run. Do not commit built plugin artifacts (`*.so` is
  gitignored), the cargo cache, or temp projects.
* There is no git-commit automation in this workspace; contributors commit
  manually after review.

## 9. Known gotchas

* `.cache/` and `.cargo-home/` are local caches used on hosts with a read-only
  `$HOME`; neither is part of the deliverable.
* Foreign availability (`python3`/`node`) and a C compiler (`cc`) determine
  which tests run; a skipped test is not a passed feature. Windows Native ABI
  is explicitly unsupported, not a pass.
* Isolated worker plugins exist (crash/address-space isolation + brokered
  `fs.read`), but they are **not** an OS sandbox. The strict sandbox and a
  package registry remain unimplemented; strict fails closed. See
  `docs/security-gate.md` and `spec/security.md`.
* Runtime errors resolve against their owning module (ADR-0017); do not
  reintroduce entry-file fallback.
* The semantic corpus may pin current quirks (for example the external-link
  call code, or a float's last digit). A fixture diff for such a quirk is a
  deliberate change, not a free update.
