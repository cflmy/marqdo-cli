# Marqdo CLI

Marqdo is a **Markdown-native, independently interpreted programming language**.
A `.mq.md` file is ordinary CommonMark/GFM prose plus explicit executable
regions: a heading declares `@exec` or `@knowledge`, `**strong**`, `*emph*` and
inline `` `code` `` are interchangeable carriers of one statement, and keywords
inside that payload — `if`, `else`, `while`, `end`, `return`, `params` — decide
control flow. Lists, block quotes and emphasis are document presentation only.
The official implementation is a Rust workspace; Python/Node are optional
foreign kernels, never the compiler target.

This repository is the new, independent Marqdo toolchain, now at product
version **`3.0.0-rc.6`** in this checkout. It is **not** a compatibility rewrite of
`cflmy/marqdo`; the old syntax, package format and plugin ABI v2 are not
claimed to work here. See [docs/developers/](docs/developers/README.md) for the
third-party entry point and `docs/MARQDO_3_0_PRERELEASE_PLAN.md` for the beta
release policy.

> **Breaking language revision landed.** The single semantic contract is
> [RFC-LANG-0002](docs/architecture/RFC-LANG-0002-unified-semantics.md), with
> normative summaries in [`spec/mapping.md`](spec/mapping.md) and
> [`spec/expression.ebnf`](spec/expression.ebnf). The frontend, expression
> parser, stdlib, examples and test corpus all implement it; the language
> migration is closed and old syntax (`**strong**` = execute, `*emph*` =
> return, `1.` = branch, `-` = loop, `>` = call) is **not** an active language.
> `marqdo_semantics`/`--legacy` selectors do not exist. See
> [`docs/STATUS.md`](docs/STATUS.md) and
> [`docs/migration/README.md`](docs/migration/README.md).

> **Current release: `3.0.0-rc.6` prerelease** —
> [release](https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-rc.6),
> source: the tagged release commit, Linux x86_64. The previous published
> release was `3.0.0-rc.5` —
> [release](https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-rc.5),
> source `c44b0f943875d563bb7c170f8ccd3fd56a82360f`. The supported prerelease
> target is **Linux x86_64 (glibc 2.39)**; macOS and Windows are experimental
> and not claimed, and the C ABI plugin/provider paths are Linux-only in this
> prerelease line. The language
> (RFC-LANG-0002), lock v4, Analysis, Observation, CLI Tool, Plugin ABI v1
> (native + worker), pinned Foreign providers and Resource Provider v2 are
> implemented and covered by tests. **`strict` OS isolation is Linux-only and
> probe-gated**; unsupported hosts must fail closed, and strict L2
> (Worker/Foreign/Provider) is not implemented. This RC records `strict:
> unsupported` with same-SHA fail-closed evidence; a disposable-VM PASS is
> required for stable/LTS, not for the RC. The broker and worker
> transports are not sandboxes. The RC release decision, support matrix and
> remaining gaps are recorded in [`docs/STATUS.md`](docs/STATUS.md),
> [`COMPATIBILITY.md`](COMPATIBILITY.md),
> [`docs/developers/release-limits.md`](docs/developers/release-limits.md) and
> [`docs/releasing.md`](docs/releasing.md). The authoritative release policy is
> [`docs/MARQDO_3_0_PRERELEASE_PLAN.md`](docs/MARQDO_3_0_PRERELEASE_PLAN.md).

CI: the source of truth is the Gitea remote
[git.cflmy.cn/cflmy/marqdo-cli](https://git.cflmy.cn/cflmy/marqdo-cli) with
Woodpecker at [ci.cflmy.cn](https://ci.cflmy.cn); [`.woodpecker/main.yml`](.woodpecker/main.yml)
runs fmt, clippy, workspace tests with `--locked`, the dependency-direction and
engine-independence gates, the plugin SDK consumption gate, the Windows
cross-target compile gate and a release build of both binaries. The strict OS-isolation lane
requires the disposable VM described in [docs/ci.md](docs/ci.md); a GitHub
mirror is not used as the authoritative lane for this prerelease line.

## Build

Requires Rust 1.89 or newer for the locked workspace.

```bash
cargo build -p marqdo-cli
# binary: target/debug/marqdo  (or $CARGO_TARGET_DIR/debug/marqdo)
```

The CLI and Provider Worker compile without any stdlib sources. The accepted
standard-library Bundle is assembled by the ecosystem repository
(`cflmy/marqdo`) from pinned component gitlinks; this repository has **no**
`stdlib/` source tree and no compile-time source fallback (ADR-0070 Phase B).
Every product test, package step and release names one explicit, verified
Bundle; `MARQDO_STDLIB_DIR` remains the explicit runtime override and the
installed `stdlib/` next to the binary is the default:

```bash
cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker
# CI/maintainers materialize the exact pinned Bundle (commit + gitlinks):
python3 tools/ci/fetch_ecosystem_bundle.py --out target/ecosystem-stdlib
python3 tools/stdlib/bundle.py --stdlib-dir target/ecosystem-stdlib --print-path
python3 tools/release/package.py --source-sha "$(git rev-parse HEAD)" \
  --version <version> --stdlib-dir target/ecosystem-stdlib \
  --bundle-pin tools/stdlib/bundle-pin.json
```

Product tests and CI set `MARQDO_TEST_STDLIB_BUNDLE` to that materialized
Bundle; without it the product lane fails closed instead of reaching a source
tree.

See [`docs/releasing.md`](docs/releasing.md) for the full procedure.

## Quickstart

```bash
marqdo init demo
cd demo
marqdo check index.mq.md
marqdo run index.mq.md            # executes ## @exec main
marqdo run index.mq.md --output jsonl        # live Observation JSON Lines
marqdo inspect index.mq.md --ast
marqdo inspect index.mq.md --signature
marqdo call index.mq.md main
```

Example project from scratch (`index.mq.md`):

````markdown
# @knowledge 数学

## @exec 阶乘

`params(n: Int)`

`if n <= 1`

`return 1`

`else`

`结果 = n * 阶乘(n=n-1)`
`return 结果`

`end`
````

```bash
marqdo check examples/factorial.mq.md
marqdo call examples/factorial.mq.md 数学.阶乘 --arg n=5   # 120
```

The parser is frozen: `marqdo.md.v1` = CommonMark 0.31.2 + GFM tables,
strikethrough, task lists + `$...$`/`$$...$$` math (comrak `=1.0.0`). A `.mq.md`
file still renders normally in any compatible Markdown reader; rendering never
executes anything. The Markdown profile is unchanged by the semantic rewrite.

## Language in 60 seconds

* **Headings declare the mode.** `# @knowledge 笔记` is a document namespace;
  `## @exec main` overrides to an executable function. Any H1–H6 may carry
  `@exec`/`@knowledge`; unmarked headings inherit the nearest ancestor. The
  document root is Knowledge. An Exec H2 is the only callable function; H3–H6
  are sections inside it.
* **Closed carriers are interchangeable.** In an Exec section, `**x = 1**`,
  `*x = 1*` and `` `x = 1` `` are the same assignment. Prefer inline code spans
  for expressions containing `*` or `**`.
* **Statements are explicit.** An `if` may have multiple `else if` arms and
  a final `else`. `while` repeats while its condition is true. `break` exits the
  nearest loop and `continue` rechecks its condition. Each carrier holds one
  statement and every `if` or `while` closes with `end` in the same heading
  body. See the [control-flow example](examples/control-flow.mq.md).
* **Containers do not compute.** `1.`, `-`, `>` and emphasis only organize the
  document. `> print(1)` is prose; `> **print(1)**` contains a call.
* **Knowledge is inert.** Nothing in a `@knowledge` section executes, so it is
  the place for normal emphasis, explanatory lists, tables and formula
  resources.
* **No syntax switch.** There is no `marqdo_semantics` field or `--legacy`
  flag; all `.mq.md` files use this syntax.

The full contract is [`spec/mapping.md`](spec/mapping.md) and
[`spec/expression.ebnf`](spec/expression.ebnf); the rationale and migration
rules are [`RFC-LANG-0002`](docs/architecture/RFC-LANG-0002-unified-semantics.md).

## What works (implemented and tested)

* **P0 contracts** — frozen `marqdo.md.v1` profile, RFC-LANG-0002 AST → program
  mapping, the v2 statement grammar and evaluation semantics; 160 AST goldens,
  73 semantic fixtures, 49 compile-error fixtures, 2 warning fixtures and 21
  runtime-error fixtures, all green (`tests/`).
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
* **Bundled stdlib + official print aliases (RFC-CM CM-A07/A10)** — one
  offline, ecosystem-assembled Bundle (component-owned module sources) with
  explicit `std:io`/`std:format`/`std:text` imports, SHA-256 digest
  verification, Analysis identity and a frozen-CLI closure test; the
  `.mq.md` print chain is the only execution path for `打印`/`print` on an official install (ADR-0031),
  covering all 15 value types; `HostOp::Print` and the legacy formatter are
  removed (ADR-0027/0028/0029/0030/0031).
* **HTTP/1.1 framing in `.mq.md`** — the `marqdo-ext-http` explicit Package
  provides status/header/chunked parsing and request construction; the CLI
  keeps the generic TCP endpoint and a loopback round-trip test (ADR-0034; no
  TLS/host HTTP claim).
* **Blocking TCP endpoint (RFC-CM P5 first slice)** — `resource.open("tcp",
  "<ip>:<port>", ["read","write"])` with exact IP grants, project mode and
  5-second timeouts; DNS/TLS/UDP/server sockets excluded (ADR-0034).
* **C ABI provider worker (RFC-CM CM-A09)** — `marqdo-provider-worker` loads a
  trusted C ABI library in a separate framed-JSON helper process (ADR-0038).
* **Provider package manifest v1 (RFC-CM CM-A09)** — local `marqdo.provider`
  manifest installs a trusted C ABI provider; no registry/signature/CLI install
  (ADR-0037).
* **Third-party Resource Provider v2 through the normal CLI (RFC-EXT-0001
  PR-03)** — a `provider.json` v2 bundle with a targeted, sha256-verified
  `c-abi-1` artifact is selected with repeatable `--provider` and authorized by
  the exact `--allow-provider` name on `run`/`call`; the existing
  `marqdo-provider-worker` transport is spawned only after every manifest,
  artifact, grant and duplicate-kind check passes. `check`/`inspect` never load
  providers, `pure`/`strict` refuse them, and the worker transport is
  linux/x86_64-only in this release (ADR-0055,
  [`spec/resource-provider-v2.md`](spec/resource-provider-v2.md)).
* **Single callable binding through the frozen composition root
  (RFC-EXT-0001 PR-01/02)** — one `CallableRegistry` and one binding pass feed
  `check`/`run`/`call`/`inspect`/Analysis; the runtime dispatches by
  `CallSiteKey` and never re-resolves a name or guesses a plugin. Static plugin
  descriptors must reconcile one-to-one with the loaded exports (ADR-0054,
  [`spec/callable-resolution.md`](spec/callable-resolution.md)).
* **Official stdlib public aliases (RFC-EXT-0001 PR-06)** — `打印`/`print` are
  verified `stdlib.json` metadata bound to `io.mq.md::写值`; the
  `print_override`/AST-scan/runtime special case is removed with byte-identical
  output (ADR-0058).
* **C ABI ResourceProvider v1 (RFC-CM CM-A09)** — trusted native shared
  libraries expose resource open/read/write/close through a frozen header;
  project + name grant; the worker transport binding is implemented (ADR-0036/ADR-0038).
* **Worker ResourceProvider transport v1 (RFC-CM CM-A09)** — trusted helper
  process with framed JSON resource operations; project + name grant; the C ABI
  library is loaded by the helper (ADR-0035/ADR-0038).
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
* **P3 project** — lock v4 full transitive package-instance graph for path and
  pinned Git sources, verified content-addressed Git cache, deterministic
  `lock`, strict `sync --locked` with atomic `.marqdo/env` materialization.
  `run`/`call`/`check` never fetch or rewrite the lock.
* **P4 security core** — capability-checked host operations, project-relative
  FS globs with traversal rejection, budgets, Observation effect events, `pure`/`project`
  modes, and **Linux `strict` mode** using Bubblewrap namespaces, no-new-privs,
  capability dropping, rlimits, a wall-clock supervisor and child
  self-verification (ADR-0021). It is probe-gated and verified in a disposable
  Ubuntu VM; unsupported hosts fail closed.
* **P5 Math/Foreign** — native inert `Formula` value from `$$...$$`; the host
  `math.*` builtin API was removed after `3.0.0-beta.1` (ADR-0061) and math
  algorithms now live in pure `.mq.md` packages or an explicitly selected ABI
  backend with the `math.compute` capability; one-shot Python/Node kernels via
  `foreign.run/call` (controlled F-A OS boundary, ADR-0063) with typed wire
  protocol, timeout and output limits; pinned provider/environment digests and
  ADR-0020 environment binding.
* **P6 plugin ABI** — frozen C ABI v1 header (`include/marqdo_plugin_abi_v1.h`),
  host bindings, Rust SDK (`mq_plugin_entry!`), `plugin.yaml` validation,
  target-triple + sha256 artifact verification, native and isolated worker
  transports, brokered `fs.read`, and explicitly trusted in-process calls
  (`--plugin --trust-native-plugin --allow-plugin`). A C ABI is not a sandbox.
* **Public protocols** — [Analysis](spec/analysis.md) projects static facts.
  [Observation](spec/observation.md) delivers the same run as live JSONL or
  a JSON snapshot. [CLI Tool](spec/cli-tool.md) installs independently built
  commands with exact protocol requirements and verifies their entire bundle.

Run everything:

```bash
cargo test --workspace --locked
cargo test -p marqdo-cli        # conformance corpus + CLI contract tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Deliberate limits (do not assume)

* **Strict is Linux-only and L1-only.** `--mode strict` enables the
  Bubblewrap supervisor only when its runtime probe passes; otherwise it exits
  1. It supports pure Marqdo arithmetic and rejects Foreign, plugins, kernels,
  external grants and live/file observation destinations (exit 2). Worker plugins provide
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
  Markdown links never call; calls are expressions inside executable payloads,
  and cross-module calls use imported alias expressions such as
  `geometry.圆面积(r=2)`.
* **`ref("#anchor")` is module-local** and errors with `ref.ambiguous` when a
  heading range contains more than one formula/code block instead of picking
  the first.
* **`fmt` is minimal** (line endings/trailing newlines), not AST-preserving
  reformatting.
* **Math is not a core API.** `math.*` builtins were removed after
  `3.0.0-beta.1` (ADR-0061); `Formula` Markdown stays inert, and math
  algorithms come from explicit pure `.mq.md` imports or a trusted ABI backend.
  The core promises no CAS/prover, solver or evaluator.
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
crates/marqdo-foreign         Python/Node one-shot kernel supervisor
crates/marqdo-sandbox         mode parsing + platform isolation detection
crates/marqdo-host-contract   neutral Host trait, HostOp, context and runtime facts
crates/marqdo-plugin-abi      public C ABI v1 contract: layout, wire envelope, errors
crates/marqdo-plugin-sdk      Rust plugin SDK and entry macro (contract-only)
crates/marqdo-host            Native Host: capability, resources, OS, plugin loader/worker
crates/marqdo-runtime         tree-walk interpreter (depends only on the neutral contract)
crates/marqdo-package         marqdo.yaml / marqdo.lock / root discovery
crates/cli                    CLI composition root (binary `marqdo`)
include/                      frozen plugin C ABI header
spec/                         normative language/format/ABI specs
docs/                         blueprint, ADRs, status, contributor guide
examples/                     runnable documents + multi-module + plugin demo
tests/                        conformance/negative corpus, embedding consumer and SDK fixture
```

## Documentation index

* [`AGENTS.md`](AGENTS.md) — operating rules for contributors and coding agents.
* [`docs/architecture/RFC-LANG-0002-unified-semantics.md`](docs/architecture/RFC-LANG-0002-unified-semantics.md)
  — **the single language authority**: full new syntax, breaking changes,
  architecture, migration and acceptance gates. Adopted by
  [ADR-0052](docs/adr/0052-unified-markdown-semantics.md).
* [`spec/mapping.md`](spec/mapping.md) — normative AST→program mapping
  (heading modes, unified carriers, control stack, diagnostics).
* [`spec/expression.ebnf`](spec/expression.ebnf) — normative statement and
  expression grammar.
* [`docs/migration/README.md`](docs/migration/README.md) — migration rules and
  the inventory of remaining work.
* [`docs/DEVELOPMENT_BLUEPRINT.md`](docs/DEVELOPMENT_BLUEPRINT.md) — architecture
  baseline (Chinese); historical sections are marked and the current language
  contract is RFC-LANG-0002, not the blueprint's old mapping.
* [`docs/MARQDO_3_0_PRERELEASE_PLAN.md`](docs/MARQDO_3_0_PRERELEASE_PLAN.md)
  — **the authoritative release policy for this line**: beta gates, version
  axes, support matrix, REL work items and LTS conditions. It is a plan, not
  evidence.
* [`docs/developers/README.md`](docs/developers/README.md) — third-party and
  integrator entry point (libraries, tools, plugins, kernels, providers,
  Analysis/Trace consumers, compatibility).
* [`docs/MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md`](docs/MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md) — beta-era integration handbook (historical; current contracts are `spec/`)
  — end-to-end third-party integration handbook.
* [`docs/STABLE_RELEASE_IMPLEMENTATION.md`](docs/STABLE_RELEASE_IMPLEMENTATION.md)
  — historical stable-release roadmap input (its own header marks it Proposed
  and superseded by the 3.0 prerelease plan for current decisions).
* [`COMPATIBILITY.md`](COMPATIBILITY.md) — protocol versions, per-platform
  feature evidence, the breaking language change and unsupported features.
* [`CHANGELOG.md`](CHANGELOG.md) — `3.0.0-beta.1` release notes plus the
  historical `0.1.0` baseline, including the syntax breaking change.
* [`docs/security-gate.md`](docs/security-gate.md) — G-C evidence and limits;
  [`docs/security-vm.md`](docs/security-vm.md) reproduces the disposable VM and
  the self-hosted security runner.
* [`docs/releasing.md`](docs/releasing.md) — artifact, checksum, clean-install
  smoke, tagging and rollback procedure.
* [`docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`](docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md)
  — historical stabilization spec (baseline `0adebe4`), superseded by the
  3.0 prerelease plan for current release decisions.
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
  broker, kernel providers, generic CLI tools, plugin-handle authority, public contract replacement).
* Specs: [`markdown-profile.md`](spec/markdown-profile.md),
  [`mapping.md`](spec/mapping.md), [`expression.ebnf`](spec/expression.ebnf),
  [`evaluation.md`](spec/evaluation.md), [`references.md`](spec/references.md),
  [`value-wire.md`](spec/value-wire.md), [`security.md`](spec/security.md),
  [`package-format.md`](spec/package-format.md),
  [`plugin-abi.md`](spec/plugin-abi.md), [`math.md`](spec/math.md),
  [`foreign.md`](spec/foreign.md), [`cli-tool.md`](spec/cli-tool.md),
  [`analysis.md`](spec/analysis.md), [`observation.md`](spec/observation.md),
  [`worker-ipc.md`](spec/worker-ipc.md);
  JSON Schemas in [`spec/schemas/`](spec/schemas/).
* [`tests/README.md`](tests/README.md) — conformance corpus and `UPDATE_GOLDENS`
  workflow.

## Examples


```bash
marqdo run examples/hello.mq.md
marqdo call examples/control-flow.mq.md main
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

### Extending Marqdo without modifying or rebuilding the CLI

RFC-EXT-0001 defines five extension kinds. Each one is discovered only through
an explicit manifest/flag, is versioned, and is authorized separately from
being visible. A physically separate repository exercising all five against a
frozen binary is `marqdo-extension-conformance`
(`https://git.cflmy.cn/cflmy/marqdo-extension-conformance`).

| Kind | Static discovery | Execution | Grant |
|---|---|---|---|
| `.mq.md` package | `marqdo.yaml` dependency + `marqdo.lock` | Marqdo interpreter | project mode |
| Callable plugin | `plugin.yaml` ABI v1 exports | worker (default) or trusted native | `--plugin` + `--allow-plugin NAME` |
| Foreign kernel | `kernel.json` + pinned executable digest | one-shot child process | `--kernel` + `--allow-foreign` |
| Resource provider | `provider.json` schema v2 | `marqdo-provider-worker` + C ABI | `--provider` + `--allow-provider NAME` |
| CLI Tool | `tool.json` install record | independent CLI process | tool installation, no project grants |

```bash
# 1. Pure Markdown library from a pinned Git dependency
marqdo lock && marqdo sync
marqdo check consumer/index.mq.md
marqdo run consumer/index.mq.md

# 2. Callable ABI plugin (worker transport by default)
marqdo run plugin/demo.mq.md \
  --plugin plugin/build --allow-plugin demo.ext

# 3. Foreign kernel with a language label the CLI has never heard of
marqdo run kernel/demo.mq.md \
  --kernel kernel/build/kernel.json --allow-foreign

# 4. Resource provider v2 (examples/provider-demo/build.sh first; see below)
marqdo run provider/demo.mq.md \
  --provider provider/build/provider.json --allow-provider demo.sensor

# 5. Independent CLI Tool consuming public Analysis
marqdo tool install tool/tool.json
marqdo inspect-ext tool/demo.mq.md
```

#### One callable resolution, visible in `check` and `run`

Calls are bound statically once per prepared program (ADR-0054, normative
details in [`spec/callable-resolution.md`](spec/callable-resolution.md)):

* `f(...)`: local callable value → official alias/reserved builtin → current
  namespace → canonical name → unique local name;
* `alias.f(...)`: explicit import alias → builtin dotted name → exact plugin
  export;
* `some_value.f(...)`: dynamic language value only.

`check` and `run` consume the same binding; an unknown export, wrong arguments
or a duplicate export fails before execution. An import alias that collides
with a builtin/plugin dotted namespace (for example importing a module as
`resource`) is rejected with `ext.import_namespace_conflict`; rename it. The
historical `math` namespace was released when the `math.*` builtins were removed
(ADR-0061), so a `math:` import alias is now accepted. A plugin
manifest is the static contract: `run` verifies the loaded artifact and its
exports/signatures match the manifest exactly
(`ext.plugin_binding_mismatch` otherwise) and never synthesises a signature.

#### Declaring extensions in the project (lock v4)

A project can declare explicit activation sources and lock their identity; a
declaration is visibility/activation, never a grant:

```yaml
extensions:
  plugins:
    - from: ./adapter/plugin.yaml
  kernels: []
  providers:
    - from: "@vendor/device/provider.json"
```

```bash
marqdo lock        # writes lock_version: 4 with manifest/artifact identities
marqdo inspect index.mq.md --extensions --json   # identity + lock status, no execution
```

Only `./relative/path` (project) and `@alias/relative/path` (declared
dependency) are accepted; absolute paths, URLs, globs and `..` are rejected.
`check`/`sync` compare the declaration with the lock
(`ext.extension_lock_mismatch`), `run` additionally re-verifies the real
artifact bytes, and `pure`/`strict` refuse project-declared third-party
extensions. Lock v3 requires an explicit `marqdo lock` migration. See
[`spec/extension-selection.md`](spec/extension-selection.md) and
[`docs/adr/0056-extension-selection-locking.md`](docs/adr/0056-extension-selection-locking.md).

#### Official stdlib aliases, no print magic

`打印`/`print` are declared in the accepted Bundle's verified `stdlib.json`
(`public_aliases`, covered by `aliases_digest`) and bound as ordinary
`Marqdo` calls to `io.mq.md::写值` (ADR-0058). There is no
`Program.print_override`, no AST-wide print scan and no runtime print branch;
output bytes, permissions and Trace behavior are unchanged, and a program that
never uses the alias does not load the stdlib module graph. Third-party
packages cannot claim `打印`/`print` or `std.*` names.

### Third-party resource providers (`provider.json` v2)

An external repository adds a new resource kind (for example `demo.sensor`)
with a `provider.json` schema-version-2 bundle and a targeted `c-abi-1` shared
library. No CLI rebuild and no core change are needed: select the manifest with
repeatable `--provider` and authorize it by its exact manifest `name` with
`--allow-provider` (also repeatable). A `--provider` without a matching
`--allow-provider` is `permission.denied` and the helper is never started;
wildcards (`*`) are not accepted.

```bash
marqdo run index.mq.md \
  --provider vendor/demo-sensor/provider.json \
  --allow-provider demo.sensor

marqdo call index.mq.md main \
  --provider vendor/demo-sensor/provider.json \
  --allow-provider demo.sensor
```

The document still uses the ordinary resource surface, e.g.
`resource.open("demo.sensor", "unit-1", ["read"])`; provider kinds do not become
new callables.

* `check`/`inspect` do not accept these options and never load a provider.
* `--mode pure` and `--mode strict` reject `--provider`/`--allow-provider` in
  preflight; strict L1 stays fail-closed and does not silently switch to
  `project`.
* Everything is verified before any worker starts: `schema_version: 2`,
  `abi: c-abi-1`, `transport: worker`, strict SemVer, non-reserved kinds, the
  current target artifact's bundle containment and sha256, exact grants and
  duplicate provider names/kinds.
* The helper `marqdo-provider-worker` must ship next to the `marqdo` executable
  for the same target (no `PATH` fallback); a missing helper is
  `ext.worker_unavailable`. The worker transport is linux/x86_64-only in this
  release; other platforms fail with `ext.platform_unsupported`.
* The helper is a trusted process, not an OS sandbox: it runs with the user's
  privileges. `sha256` is an integrity check, not publisher authentication.
  See [`spec/resource-provider-v2.md`](spec/resource-provider-v2.md) and
  [ADR-0055](docs/adr/0055-resource-provider-cli-v2.md).

## References

* [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) ·
  [GFM](https://github.github.io/gfm/) · [Comrak](https://comrak.ee/)
* [Cargo](https://doc.rust-lang.org/cargo/) · [uv](https://docs.astral.sh/uv/)
* [Wasmtime Component Model](https://component-model.bytecodealliance.org/)
* Legacy Marqdo source (reference only): <https://github.com/cflmy/marqdo>
