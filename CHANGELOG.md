# Changelog

All notable changes to the Marqdo CLI workspace. Product version
`3.0.0-beta.1` was tagged on 2026-11-22 at source
`2c7c4f31e7a388d3fcfa717a77067e696ed13854`
([release](https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-beta.1));
the next section collects post-beta
changes. Current protocol contracts are Analysis, Observation, CLI Tool,
Plugin ABI v1, Foreign wire v1, Provider manifest v2 and lock v4; independent
package versions (including the bundled stdlib `0.1.0`) are versioned
separately and do not follow the product version. Dates use ISO 8601.

## 3.0.0-rc.3 (candidate)

- The Rust workspace, CLI version output and protocol producers report
  `3.0.0-rc.3`. The locked workspace requires Rust 1.89 and targets Linux
  x86_64 on glibc 2.39.

### Single current public contracts — issue #8 (breaking)

Executed policy: [ADR-0068](docs/adr/0068-single-current-public-contracts.md) and
[the protocol convergence policy](docs/architecture/PROTOCOL_CONVERGENCE_POLICY.md).
This is a pre-stable replacement, not a compatibility release. Retired formats
are rejected; there is no legacy reader, alias, fallback, negotiation or
automatic migration.

- **Analysis has one writer for every project.** `check --emit` always emits
  `marqdo.analysis` `schema_version: 3`, including projects without
  `break`/`continue`. The content-driven v2/v3 selector is deleted, and the
  retired `spec/analysis-v*.md` documents and schemas are removed from the
  current tree. Consumers no longer need to branch on the project content.
- **Trace v2 and Execution Events v1 became one Observation contract.**
  `run`/`call` emit one `marqdo.observation` `schema_version: 1` event sequence
  per execution. `--output jsonl` streams those events, `--output json` and
  `--observation-file PATH` publish an atomic snapshot of the same events from
  the same execution. `run_id`, `seq`, frame identity, terminal semantics,
  capture budgets and privacy rules are defined once in
  [`spec/observation.md`](spec/observation.md). The retired `--json`,
  `--trace-file` and `--event-stream` options are rejected.
- **CLI Tool requirements are single exact values.** `tool.json` declares
  `requires.api_revision` (required) plus optional exact `analysis` and
  `observation` integers. Version arrays and intersection negotiation are
  removed. Retired manifests and records are rejected with a remove/reinstall
  instruction and are never rewritten.
- **Plugin and resource-provider workers share one transport.** Framing,
  bounded reads, deadlines, dead-worker quarantine and teardown live in
  `marqdo-worker-ipc` ([`spec/worker-ipc.md`](spec/worker-ipc.md)); the invoke
  vocabulary, grants, kind rights and error codes stay with each boundary.
- **Extension descriptors and lock keep their current identifiers.** There is
  no version renumbering without a shape change: plugin `abi: ^1.0`, provider
  `schema_version: 2`, kernel `wire_version: 1` and lock v4 stay current.
  Retired provider/kernel/plugin revisions and lock v1–v3 are rejected with an
  explicit regeneration/reinstall instruction; no migrator runs.
- **`spec/` now contains only current contracts.** Retired specifications are
  available in Git history and tagged archives. ADRs covering retired public
  contracts carry a Historical header.

Migration for consumers:

1. Re-run `marqdo check --emit` and read `marqdo.analysis` without a version
   branch; verify `project`, module ids and source digests.
2. Replace Trace/Event readers with an Observation reader; stream with
   `--output jsonl` and/or read `--observation-file`. Verify `run_id`, `seq`,
   source binding and the single terminal event.
3. Rewrite `tool.json` to the exact current requirements, rebuild the tool and
   `marqdo tool remove` + reinstall. Old registrations are not migrated.
4. Rebuild/reinstall plugins, providers and kernels from their current
   descriptors; re-run `marqdo lock` instead of reusing a retired lock.

### Verification

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace --locked` pass on the candidate tree.
- Independent consumers (`tests/consumers/analysis_consumer.py`,
  `observation_consumer.py`, `validate_protocols.py`) validate real CLI output;
  `tests/consumers/observation_consumer.py` checks that stream and snapshot are
  the same event sequence.
- `crates/cli/tests/architecture_boundaries.rs` forbids a second worker
  framing implementation and pins the workspace layer list;
  `docs_consistency` keeps one current overview and resolving links.
- Same-SHA Linux CI [pipeline #70](https://ci.cflmy.cn/repos/1/pipeline/70)
  passed all lanes, produced archive sha256
  `d6ef6297a7ba5297af758a0b00da5c6e0f4b69f40363afc4ddc180654e70ac37`, passed
  the from-archive smoke and the frozen external conformance suite 7/7 with
  fixtures at `015ff8ac15417b7ebbdb31ce2be2dab5362a1eba`.
- Publication still requires the RC strict-VM and G4 performance evidence
  described in [`docs/releasing.md`](docs/releasing.md).

## 3.0.0-rc.2 (candidate)

- The Rust workspace, CLI version output and protocol producers report
  `3.0.0-rc.2`. The locked workspace requires Rust 1.89.
- The candidate targets Linux x86_64 with glibc. See
  [release gates](docs/releasing.md) for the evidence required for publication.

### Structured control flow (RFC-CTRL-0005)

- Added ordered `else if` arms and nearest-loop `break` and `continue`.
  Bare keyword carriers take the control meaning. Calls, bindings and
  identifiers within expressions retain their existing syntax.
- Inferred signatures now use reachable reads and merge assignments on
  reachable control exits. A parameter inferred only from unreachable
  statements may disappear.
- Analysis v3 represents loop transfers. Projects without transfers still emit
  v2. Trace v2 and Execution Events v1 report the two explicit control events.

### Core/extension boundary — RFC-EXT-0003 (issue #3)

Executed evidence: [`docs/evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md`](docs/evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md).
Frozen decisions: ADR-0060 (heading modes/annotations), ADR-0061 (math
detach), ADR-0062 (resource-operation ownership/core kinds), ADR-0063
(foreign F-A transition).

#### Breaking changes (beta line; migrate before the next beta/stable)

- **`math.*` core builtins removed.** `math.parse`, `math.eval`,
  `math.substitute`, `math.simplify` and `math.differentiate` no longer exist in
  `BUILTINS`, `HostOp`, the runtime or the host. A document that calls one now
  fails statically with `symbol.unknown`; there is no `math.unsupported` runtime
  fallback and no old-name compatibility parser. `crates/marqdo-math` is removed
  from the workspace and host dependency graph. `Int`/`Float` arithmetic and the
  inert Markdown `Formula` value stay. Math algorithms move to pure `.mq.md`
  packages (`examples/math-library/`) or an explicitly selected ABI backend with
  the `math.compute` capability (`examples/math-backend/`). The historical
  `math` import namespace is released, so `import: { math: ... }` is accepted
  again. Migration: [`spec/math.md`](spec/math.md).
- **Sibling heading inheritance bug fixed (H-06).** `build_units` now removes the
  current and same-or-higher-level headings from the ancestor stack *before*
  deriving the inherited mode. An unmarked heading can no longer inherit the
  previous sibling's mode: under `# @knowledge` a `## @exec` sibling followed by an
  unmarked `##` leaves the latter Knowledge/inert; under `# @exec` a `## @knowledge`
  sibling followed by an unmarked `##` makes the latter Exec/callable. This is a
  correctness fix with new conformance/black-box/`check --emit` coverage.
- **Release archive now ships the verified `stdlib/`.** The archive contains
  `stdlib.json` and every `.mq.md` module; `RELEASE_INFO.json` records name,
  version, tree digest, alias digest, count and file list. `package.py` recomputes
  both digests and refuses a mismatch; `smoke.py` re-verifies them and fails
  closed when the bundle is removed. Release builds no longer fall back to the
  compile-time source tree (debug builds keep a development-only fallback). An
  independent `ubuntu:24.04` cold-install container check is recorded in the
  evidence.
- **Core resource kinds are one registry (H-04).** `marqdo_host::CORE_RESOURCE_KINDS`
  (`stdio`, `file`, `tcp`, `tls`, `udp`, `tcp-listener`) is now used by both the
  CLI provider-manifest validator and every trusted host registration path; a provider
  name or declared kind equal to a core kind is rejected before registration.
- `resource_request` operation ownership is reviewed and enforced by an
  architecture gate. New domain operations are forbidden in the core; see
  [`docs/architecture/resource-operations.md`](docs/architecture/resource-operations.md)
  and ADR-0062.

#### Proposed, not implemented

- **Heading Annotations v1 (H-05/H-07/H-13).** `@exec`/`@knowledge` are frozen
  as the only Core Modes and alone decide execution; a declarative,
  metadata-only `@namespace.identifier` annotation layer is specified as
  **Proposed** with target `3.1.x`. The current parser still rejects any other
  leading `@foo` with `sem.invalid_mode_marker`; no `annotation.unknown` code,
  manifest loader or heading annotations projection exists in this build. Contracts:
  ADR-0060 and [`spec/heading-annotations-v1.md`](spec/heading-annotations-v1.md).
- **Foreign F-B removal is deferred (H-02).** The permission-gated one-shot
  `foreign.run`/`foreign.call` + `KernelRegistry` stay as the controlled
  OS-boundary primitive (F-A); removal needs a versioned generic process/worker
  capability and the existing security negatives. ADR-0063 and
  [`spec/foreign.md`](spec/foreign.md) §8.

### Runtime observability — Execution Events v1 (RFC-OBS-0001, ADR-0059)

- `marqdo run` and `marqdo call` accept `--event-stream jsonl`: the CLI stdout
  becomes a pure NDJSON `marqdo.execution-events` v1 channel. Program
  stdout/stderr are carried as byte-exact base64 chunk events emitted at the
  Host commit point, not by splitting the final buffer.
- Lifecycle and terminal events (`stream.opened`, `run.started`,
  `run.completed`, `run.failed`, `run.rejected`) have defined ordering and exit
  codes. `--event-stream` is mutually exclusive with `--json`, is rejected by
  static commands and by `--mode strict`, and may be combined with
  `--trace-file`; the two writers share one run id and Analysis digest.
- Call instances get run-unique `frame_id`/`parent_frame_id`; `call.enter`,
  `call.exit`, `return`, `branch.eval`, `loop.iteration`, `effect` and
  `variable.assigned` events are bounded by a structural budget with a single
  `observation.truncated` marker.
- Values are off by default. `--observe-variable` / `--observe-parameter` are
  exact-name selections; `Int`/`Float`/`Bool` are bounded and `Text` requires
  the separate `--observe-text-values` consent. Unselected, redacted,
  truncated and unsupported states are distinct.
- New public contract
  [`spec/execution-events-v1.md`](spec/execution-events-v1.md) and
  [`spec/schemas/execution-events-v1.schema.json`](spec/schemas/execution-events-v1.schema.json),
  with fixtures and an independent Python consumer. Trace v2 retains its
  public event envelope.
- Limitations: memory metadata events (OBS-06/P2), a bidirectional debug
  protocol and SSE/HTTP remain out of scope; strict mode refuses event mode
  explicitly.

## 3.0.0-beta.1 (2026-11-22)

First public prerelease of the independent Marqdo toolchain. This is a
**prerelease, not stable and not LTS**; the support claim is Linux x86_64 until
the stable `3.0.0` gate passes. The complete beta policy, gates and deferred
items are in
[`docs/MARQDO_3_0_PRERELEASE_PLAN.md`](docs/MARQDO_3_0_PRERELEASE_PLAN.md) and
[`docs/developers/release-limits.md`](docs/developers/release-limits.md).

### Product version and compatibility

- Workspace/crate product version, `marqdo --version` and Analysis
  `producer.version` are now `3.0.0-beta.1`. `Cargo.lock` is committed and all
  release builds use `--locked`.
- `project.marqdo` in `marqdo.yaml` is now **enforced** against the running
  core instead of being informational. Incompatible ranges fail closed with
  `package.marqdo_incompatible`; malformed ranges fail with
  `package.marqdo_range_invalid`. The `init` template writes
  `>=3.0.0-beta.1,<4.0.0`.
- SemVer prerelease negotiation is covered by tests: a stable-only range such
  as `>=3.0.0,<4.0.0` does **not** silently accept a beta core, while an
  explicit `>=3.0.0-beta.1,<4.0.0` Tool/Project range does.
- The official bundled stdlib stays `0.1.0`; plugin, tool and example package
  versions are independent of the product version. No protocol major is
  renumbered because the product is 3.0.

### Release engineering

- Release archives now ship **both** `marqdo` and `marqdo-provider-worker` in
  the same directory for the same target triple, plus `RELEASE_INFO.json`
  (source SHA, target, toolchain, binary SHA-256), `SOURCE_SHA`,
  `KNOWN_LIMITATIONS.md`, the public specs, developer docs, runnable examples
  (including host-built plugin/provider artifacts) and the Analysis/Trace
  consumers. `tools/release/package.py` builds deterministically;
  `tools/release/smoke.py` verifies a cold install from the archive only.
- Release evidence distinguishes the **beta** profile from the stable profile:
  `tools/ci/release_evidence.py --profile beta` requires Linux, strict-policy,
  frozen-binary extension, clean-install and version-compatibility gates,
  records unsupported platforms explicitly and never turns a missing or
  SHA-mismatched gate into a release.
- Added a runnable Resource Provider v2 example
  (`examples/provider-demo/`) and used the frozen external conformance
  repository as the same-SHA black-box gate.

### Developer entry points

- New [`docs/developers/`](docs/developers/README.md): quickstart, `.mq.md`
  libraries, Tool Protocol, extensions (plugin/kernel/provider), Analysis/Trace
  consumers, compatibility and release limits.
- The third-party integration handbook was corrected and exercised against the
  released binary; `spec/tool-v1.md` no longer says Proposed,
  `spec/plugin-abi.md` no longer says native-only, and `spec/package-format.md`
  documents the enforced `project.marqdo` field and lock v4.
- `check --emit` help now correctly says Analysis v2.

### Support policy

- Supported release platform: **Linux x86_64 (glibc)**. macOS/Windows are
  experimental and not claimed; the Windows cross-target check remains a CI
  compile gate only.
- `strict` policy for this beta is recorded with same-SHA evidence in
  [`docs/STATUS.md`](docs/STATUS.md); unsupported hosts fail closed. strict L2
  is unsupported and refused.
- The G4 performance gate is recorded as a Beta-only WARN on the fixed
  Woodpecker agent, with environment and limits, instead of being promoted to a
  stable PASS.

## Unreleased (post-beta)

### Planned changes

- Fixes collected from the first beta users; see `docs/MARQDO_3_0_PRERELEASE_PLAN.md`
  §5 for the conditional `3.0.0-beta.2` / `rc.1` line.

## Historical: stabilization candidate

### Breaking changes — extensibility closure (RFC-EXT-0001)

Implements [`docs/architecture/RFC-EXT-0001.md`](docs/architecture/RFC-EXT-0001.md)
(ADR-0054/0055/0056/0057/0058). These are the 3.0 breaking changes; the
independent protocol versions listed above are unchanged:

- **One callable resolution.** `check`/`run`/`call`/`inspect`/Analysis share a
  single registry and binding pass (`CallSiteKey`). The runtime no longer
  re-resolves names or guesses a plugin when no descriptor is loaded. An import
  alias that collides with a builtin/plugin dotted namespace (for example
  importing a module as `math`) is now `ext.import_namespace_conflict`; rename
  the alias. Unknown plugin exports are `symbol.unknown` at compile time.
- **Exact plugin binding.** A plugin manifest is the static signature contract;
  `run` verifies artifact bytes and reconciles exports/signatures/owners/digests
  one-to-one (`ext.plugin_binding_mismatch`). Empty/synthesised signatures are
  gone; ABI v1 signature defaults are rejected; positional arguments are
  normalised to manifest order.
- **Lock v4.** `marqdo.lock` upgrades to `lock_version: 4` with an `extensions:`
  identity table. Lock v3 is rejected with an explicit `marqdo lock` migration
  error; it is never reinterpreted silently. Projects can declare explicit
  activation sources under `marqdo.yaml.extensions` (`./path` or `@alias/path`
  only); a declaration is not a grant.
- **Analysis calls projection.** `calls[].state/target` now come from the same
  binding; official stdlib aliases such as `打印` project as `function` targets.
  Analysis v2 schema/version are unchanged and the private binding table is not
  serialized.
- **Print magic removed.** `打印`/`print` are verified `stdlib.json`
  `public_aliases` bound to `io.mq.md::写值`; `Program.print_override`, the
  AST-wide print scan and the runtime print branch are deleted. Output bytes are
  unchanged; programs without print do not load the stdlib module graph.
- **Third-party resource providers via CLI.** `--provider PATH` /
  `--allow-provider NAME` on `run`/`call` select and authorize a
  `provider.json` schema-2 worker artifact; verified before spawn; pure/strict
  refuse; linux/x86_64 only (other platforms fail closed).
- **Capability handler registry.** Granted capabilities execute only through a
  trusted installed handler under budget/audit; a declared-but-unhandled
  capability stays `capability.unavailable`.

### Breaking changes — language source syntax (RFC-LANG-0002)

**The `.mq.md` source language changed incompatibly. Old source files are not
executable.** The current contract is
[`docs/architecture/RFC-LANG-0002-unified-semantics.md`](docs/architecture/RFC-LANG-0002-unified-semantics.md)
(accepted by [`docs/adr/0052-unified-markdown-semantics.md`](docs/adr/0052-unified-markdown-semantics.md)),
summarized normatively in [`spec/mapping.md`](spec/mapping.md) and
[`spec/expression.ebnf`](spec/expression.ebnf).

- Removed the old mapping in which `Strong` executes, `Emph` returns, an
  ordered list is a branch, a bullet list is a loop, a block quote is a call and
  the first standalone code span is a signature.
- Headings now declare `@exec` / `@knowledge`; unmarked headings inherit the
  nearest ancestor's mode, and the document root is Knowledge.
- Only an Exec H2 is a callable function; H2 Knowledge is inert and H3–H6 are
  non-callable sections. A payload with no owning Exec H2 is
  `sem.exec_without_function`.
- `Strong`, `Emph` and inline `Code` are interchangeable carriers of exactly one
  statement; the AST kind no longer selects the operation.
- Control flow comes from keywords in the payload: `if`/`else`/`while`/`end`
  (explicitly closed) plus `return <expr>` and `params(...)`. Ordered lists,
  bullet lists, block quotes and paragraphs are presentation containers with no
  instruction type; a plain list or quote produces zero statements.
- `params(...)` is the only function-signature declaration, legal only as the
  first direct payload of an Exec H2. An Emph/Strong payload in a signature
  position is no longer special.
- There is no syntax version switch, per-file selector, `--legacy` path or
  fallback mapper; `marqdo_semantics`/syntax-selection front matter is rejected.

**Historical implementation record (at `9c1723d`).** The language migration
landed in the `semantics-v2-refactor` history (docs/spec `4983542`,
frontend/runtime/stdlib/corpus `e029853`, expression-owned statement grammar
`ffd1cc9`, then the follow-up fixes through `9c1723d`). At `9c1723d` the
workspace gate was green (507 passed, 0 failed, 2 ignored). That count is
history; the current release-SHA gate is recorded in
[`docs/STATUS.md`](docs/STATUS.md). See also
[`docs/evidence/2026-11-21-semantics-v2-green.md`](docs/evidence/2026-11-21-semantics-v2-green.md).

**Follow-up correctness fixes in the same series:**

- `params(...)` is enforced as the first function-wide direct Exec H2 payload, so
  a later `### @exec` segment cannot add a signature after earlier statements;
- a `params(...)` code span in a `@knowledge` section is inert documentation and
  never becomes a signature (`inspect --signature` now reports `inferred`);
- a stray `end` diagnostic points at the `end` payload instead of the previous
  statement;
- Setext headings inherit but never declare a mode, as `spec/mapping.md` states;
- diagnostics are emitted in the documented stable order (source position, then
  code);
- the unreachable `Stmt::Call`/`CallSite`/`CallTarget` IR left over from the
  removed link-form block-quote call was deleted.

**Required migration actions for consumers and the repository:**

1. Rewrite every active `.mq.md` source by AST role (not by a global regex):
   stdlib modules, examples, fixtures, project templates, Rust-embedded samples
   and documentation snippets.
2. Give callable functions an explicit `## @exec 名称` heading, or place them
   under an `# @exec` namespace. Knowledge documents and visual emphasis belong
   in `@knowledge` sections; a `Strong`/`Emph` payload in an Exec section is a
   statement and may fail as an unknown symbol.
3. Replace italic returns with `return <expr>` in any carrier.
4. Convert an ordered-list branch to a first-true nested `if`/`else`/`end`
   chain; convert each bullet-list item to its own `while`/`end` loop, preserving
   item order and early-return behavior.
5. Convert block-quote calls to payload calls (`**print(x)**`,
   `` `print(x)` `` or `> **print(x)**`) and standalone code-span signatures to
   `params(...)` as the first direct Exec H2 payload.
6. Replace old shape expectations with the `sem.*` diagnostics in
   `spec/mapping.md` §8; ordinary lists/quotes are now valid inert Markdown.
7. Regenerate `stdlib/stdlib.json` only in the same change that migrates the
   stdlib sources, and never use `UPDATE_*_GOLDENS=1` to mask a semantic change.
8. A dependency or lock built from old source syntax is not compatible; relock
   and re-verify the package instance identity after migration.

### Added

- `.gitattributes` byte-identity policy: canonical LF for text files and one
  explicit CRLF byte fixture for Markdown line-ending semantics.
- Byte-level AST golden diagnostics: first differing byte, byte windows and an
  explicit "only line endings differ" classification.
- Cross-platform Native ABI test fixtures generated for the host target, plus
  `no_matching_artifact`, artifact-tamper and ABI-major-mismatch negative tests.
- Host-specific example plugin builds under `examples/plugin-*/build/`; the
  tracked manifests are never rewritten by tests or CI.
- Foreign pinned-environment binding (ADR-0020): an external provider's
  `entrypoint` must resolve inside `environment_dir`, built-in providers cannot
  be bound to one, the child gets only the environment executable directory on
  `PATH`, and `interpreter_version` is verified before execution.
- Linux strict isolation (ADR-0021): a Bubblewrap supervisor with user/mount/
  PID/net/UTS/IPC namespaces, no-new-privs, capability dropping, a read-only
  `/project`, rlimits, output/wall-clock supervision and child
  self-verification. Probe-gated; L1 supports pure Marqdo + Math only.
- Adversarial G-C lane (`tests/security/escape_helper.c`,
  `crates/marqdo-sandbox/tests/security_linux.rs`, `tests/security/run-lane.sh`)
  covering filesystem, `/proc`, network, process, privilege and resource
  escapes. 7/7 PASS in a disposable Ubuntu 26.04 VM.
- Release documentation (`COMPATIBILITY.md`, `docs/releasing.md`) and the
  Linux security-gate evidence report (`docs/security-gate.md`).
- `check_never_spawns_a_foreign_kernel`: proves static `check` does not spawn a
  declared Foreign provider, while `run` on the same document does.
- Power/control-flow corpus: 10 new AST goldens, 8 semantic goldens, 4
  compile-negative fixtures and 2 runtime-negative fixtures covering
  right-associative `**`, unary-minus precedence, parentheses, float negative
  exponents, Strong+CodeSpan payloads, branch/loop conditions, inert prose and
  code fences, and source spans inside one/two-backtick code spans. (The
  fixtures were migrated to the unified payload/keyword syntax and are green at
  `9c1723d`.)
- Independent Tool Protocol v1 consumers `sample-view`, `sample-knowledge` and
  `sample-proof` (analysis/trace readers) with install, dispatch and remove
  tests; none is a hard-coded CLI command.
- Locked path-dependency content-tamper test: a changed dependency is rejected
  without rewriting `marqdo.lock` or the materialized `.marqdo/env`.
- G-C lane honours `CARGO_TARGET_DIR`, so a run from a clean `git archive` with
  an external target directory uses the binary it just built.
- Addressable `Memory` (RFC-CM-0001 P1, ADR-0022): an opaque run-scoped
  region value with 64 KiB zero-filled pages, grow-only capacity, bounded
  byte read/write, `memory.new/size/grow`, and `m[offset] = byte`. Copies share
  the region identity; different regions never alias. Default budgets are
  64 MiB committed per run and 16 MiB per region; `--max-memory` and
  `--max-region-memory` override the budgets on `run`/`call`.
- Memory-index place assignment in the expression/IR model
  (`AssignTarget::Index` / `PlaceTarget::Index`) with the frozen evaluation
  order base, index, RHS and no change to `List`/`Map`/`Table` value semantics.
- Stable `runtime.memory` error category for invalid/stale handles and page
  counts; `runtime.index` for offsets, `runtime.type` for operand/byte-range
  errors, `resource.exhausted` for budgets.
- Analysis v2 projects an index assignment target as an additive
  `{"kind":"index",...}` object while local targets keep the v1 string.
- `examples/memory-buffer.mq.md`, semantic goldens 45–47 and runtime negatives
  13–18 cover sharing, isolation, growth, zero-fill, offsets, byte range,
  non-Memory index assignment, declared-max growth and negative page counts.
- Partial RFC-CM P2 closure evidence (`.mq.md` only, no Rust algorithm
  additions): fixtures 48–50 implement a big-endian u32 codec, a bump
  allocator, node allocation and a linked-list traversal/sum, and prove that a
  failed allocation leaves the allocator header unchanged. This is **not** the
  full P2 gate: a separate stdlib package and a frozen-CLI closure lane are
  still missing.
- `Memory` handles are never transferable: `to_wire` emits a bare rejected tag,
  `from_wire` fails, and every host/plugin/foreign call rejects arguments that
  contain a handle.
- Minimal structure observation/construction (RFC-CM CM-A04, ADR-0023):
  `list.append/concat`, `map.keys/has/get/set/remove`,
  `text.codepoints/from_codepoint`, and `table.columns/row/cell`. All are pure,
  value-returning builtins; inputs are never mutated and no new syntax is
  added. Semantic fixtures 51–53 and runtime negatives 19–21 cover value
  semantics, Unicode scalars, table projection, wrong types and bounds.
- `.mq.md` table filter (`过滤`) built on `table.columns/row/cell`,
  `list.append` and `表`, with a semantic golden showing the input table is
  unchanged (fixture 54). No `table.filter`/`HostOp` is added.
- Recoverable `Result` convention (RFC-CM CM-A05, ADR-0024): an ordinary Map
  `{ok:true,value}` / `{ok:false,code,message[,details]}`, constructed by
  `.mq.md` functions, with expected failures recoverable and language
  invariant violations still fatal. No `Result` type, exception or
  `try`/`catch` syntax.
- `.mq.md` first-fit free-list allocator (fixture 55): 8-byte block headers,
  free-block reuse, block splitting, address-adjacent coalescing (tested in a
  second region whose bump space is exhausted, so only a merged block can
  satisfy the request), non-overlapping reused payloads and `alloc.no_space`
  returned as a `Result` without aborting the run. No allocator Rust builtin or
  `HostOp` was added.

- `.mq.md` P2 closures beyond the first slice: binary tree insert/in-order
  traversal (56), CSV row splitter (57), open-addressing hash table (58),
  UTF-8 encoder/validating decoder (59–60), graph adjacency list (61),
  recursive JSON parser for arrays/objects/strings (62), stack bytecode VM
  (63), 16/32/64-bit + LEB128 codecs (64), table sort (65), table equality join
  (67) and quoted/escaped CSV (68). No Rust algorithm, container or parser was
  added for these; algorithms remain `.mq.md` code. JSON float parsing and a
  compact formatter are included.
- Resource/stream first slice (RFC-CM CM-A06, ADR-0025): opaque host-issued
  `Resource` handles, `resource.open/read/write/close`, bytes transferred
  through `Memory`, stdout write in pure/project under console/output budgets,
  granted project-file chunked read in project, EOF as success, closed-handle
  Result errors, forged-handle fatal errors, wire/host-boundary rejection, and
  strict refusing every resource open. Semantic fixture 66 and host/CLI tests.
- Buffered project-file write streams (ADR-0026): `file` + `["write"]` requires
  an `fs_write` grant, buffers in the host, flushes only on `close`, never
  flushes on drop, has an 8 MiB ceiling, and is refused in pure/strict. A
  `.mq.md` chunked copy loop reproduces a file without a Rust `copy_file`.
- Compat-C print completion (ADR-0031): remove `HostOp::Print` and
  `StdHost::op_print`; the print builtin only routes to bundled `io.写值` or
  returns `kernel.unavailable`; print traces contain only resource effects.
- Compat-B `打印`/`print` routing (ADR-0030): a `Program.print_override` sends
  print calls to bundled `io.写值` when the program uses them; stdlib modules
  are injected lazily, resource operations appear as trace effects and the
  legacy `HostOp::Print` path does not run. Strict permits only stdio/stdout
  (existing console), not file/network resources. No Compat-C removal yet.
- Full-value stdlib formatting (ADR-0029): FunctionRef `module/name` and
  PluginHandle `plugin/export` read-only members (host id private);
  `stdlib/format.mq.md` now reproduces builtin `打印`/`repr` for all 15 value
  types, including Formula/Code and UTF-8 byte length for Code.
- CM-A10 bundled stdlib (ADR-0028): a fixed `stdlib/` tree with
  `format/text/io`, explicit `std:<path>` imports, SHA-256 digest verification
  before module loading, a `std:marqdo-stdlib@<version>#<digest>` package
  instance id, `project.stdlib` in Analysis v2, and test coverage for
  `std:` resolution, tamper rejection and frozen-binary hash equality.
- P2 closure gate: add a pure no-Foreign/no-Plugin lane for C01–C06/C11/C12
  (20 fixtures + stdlib table), source shortcut scan and golden comparison.
- CM-A10 offline packaging: `tools/pack-stdlib.py` emits a deterministic
  `marqdo-stdlib-0.1.0.tar.gz` plus tree/archive digests and a file manifest;
  the archive is for the future external repository, not the CLI loader.
- P2 anti-evasion: explicit pure-lane negative tests prove `foreign.call` is
  `permission.denied` and `plugin.invoke` fails, alongside the 21 fixture lane.
- C12 sparse matrix: fixture 73 adds triple-list sparse multiply and dense
  conversion, included in the P2 pure closure lane.
- P6 baseline profile: add `tools/perf/bench.py` and a checked-in 7-workload
  JSON report; no bytecode/JIT/native backend is introduced.
- P4 cross-storage table equivalence: fixture 72 compares row-store Table
  sort/filter output with a `.mq.md` columnar store cell-by-cell.
- CM-A09 C ABI provider worker binding (ADR-0038): add the separate
  `marqdo-provider-worker` helper that loads a trusted C ABI library and serves
  the ADR-0035 framed-JSON protocol; crash isolation but not a sandbox.
- CM-A09 provider package manifest v1 (ADR-0037): local manifest resolves a
  trusted C ABI library and registers it; no registry/signatures/CLI install.
- CM-A09 C ABI provider v1 (ADR-0036): trusted shared libraries implement
  open/read/write/close/version/last_error; host loads them with version and
  symbol checks, project-mode + name grant only. Plugin-Worker binding remains
  open.
- CM-A09 worker provider transport (ADR-0035): host-managed helper process
  speaking framed JSON for resource open/read/write/close; trusted-only,
  project-mode + name grant; C ABI binding still open.
- P5 HTTP framing/client: `stdlib/net/http.mq.md` parses status/headers/chunked
  responses and builds requests; `http_over_tcp.rs` runs a loopback TCP HTTP
  exchange through `resource.open("tcp")`. No host HTTP/TLS implementation.
- CM-A08 GFM rendering: `stdlib/data/table.mq.md::转GFM` renders a logical
  Table as a GFM pipe table with cell escaping; CLI test added.
- P5 HTTP parser fixture (C10): semantic fixture 71 parses HTTP/1.1 status,
  headers and chunked transfer bodies and builds requests in `.mq.md`; no host
  HTTP/HTTPS implementation is claimed.
- P5 blocking TCP endpoint v1 (ADR-0034): `resource.open("tcp", ip:port,
  ["read","write"])` with exact IP-literal grants, project mode only, 5s
  connect/read/write timeouts; no DNS/TLS/UDP/server sockets. CLI `--allow-net
  <IP>`; loopback host and `.mq.md` echo tests. Strict/pure refuse.
- CM-A09 in-process ResourceProvider v1 (ADR-0033): `ResourceProvider` trait,
  `StdHost::register_resource_provider`, per-kind registration with protocol
  version/conflict checks and `Capabilities.allow_resource_provider`; provider
  streams reuse `resource.*` and are project-mode only. A mock read/write
  provider test runs without parser/runtime/`HostOp` changes. C/Worker provider
  transport remains open.
- CM-A08 table logical/stdlib slice (ADR-0032): `stdlib/data/table.mq.md`
  adds select/filter/sort/append/set-cell/join/group as value-returning
  `.mq.md` functions; `Value::Table` semantics and GFM AST laziness unchanged;
  no physical storage migration or new table builtin.
- CM-A07 **Compat-A** prototype (ADR-0027): `stdlib/` provides
  `format.显示`, UTF-8 encoding and `io.写值` (`resource.write(stdout)`), and
  its output is comparison-tested byte-for-byte against builtin `打印` for
  Unit/None/Bool/Int/Float/Text/List/Map/Table. Builtin `打印` remains the
  default; no alias routing, bundled stdlib or `HostOp::Print` removal; the
  other value types lack public observation.

### Changed

- `spec/mapping.md` and `spec/expression.ebnf` were replaced in place by the
  RFC-LANG-0002 contract (heading modes, unified payload carriers, keyword
  control flow). `spec/markdown-profile.md` remains `marqdo.md.v1`; only the
  Markdown→program mapping changed. The frontend, expression parser, IR,
  runtime and bundled stdlib implement these specs in the same tree; see
  "Breaking changes" above.
- Assignment targets are no longer restricted to bare identifiers: a single
  index place is accepted at statement level. Member assignment stays a parse
  error; chained assignment stays a parse error. `spec/expression.ebnf`,
  `spec/evaluation.md`, `spec/value-wire.md`, `spec/analysis-v2.md` and
  `spec/security.md` document the new semantics.
- `docs/MARQDO_3_0_PRERELEASE_PLAN.md` is the current release policy;
  `docs/STABLE_RELEASE_IMPLEMENTATION.md` is historical roadmap input, not
  current status.
- AST conformance tests fail early when a golden contains CR bytes instead of
  reporting an opaque mismatch.
- Worker plugin fixtures use the platform dynamic-library name and compiler
  flags.
- List-item condition paragraphs now accept only inert description; an
  executable `Strong`/`Emph` there is a hard `markdown.invalid_construct`
  error instead of being silently swallowed. Condition expression spans are
  mapped from the literal bytes inside the code-span backticks. (Pre-RFC
  mapping; superseded by RFC-LANG-0002, where lists have no condition head at
  all.)
- Linux strict `inspect --platform` remains a snapshot, not a promise. A run
  that loses namespace/resources between probe and launch must fail closed.

### Fixed

- Windows CI AST golden failure caused by CRLF checkout of canonical JSON
  goldens (reproduced with a fresh `core.autocrlf=true` clone).
- macOS CI plugin failure caused by assuming a Linux `.so` and Linux-only
  manifest target.
- macOS/Windows `E0433`: platform detection now uses `#[cfg]` functions
  instead of `cfg!` around `linux::probe()`, so the Linux module is not
  referenced on other targets.
- Linux strict probe→launch race: the CLI contract and the sandbox seam now
  require a non-zero, diagnosed, fail-closed refusal when the real supervisor
  launch fails after a successful probe.
- Parallel security tests shared process-id scratch paths and deleted each
  other's project; every case now has a unique path and the helper is compiled
  once.
- Condition parse diagnostics pointed at the backtick instead of the missing
  operand; the payload map now excludes delimiters.

### Security

- Linux `strict` is implemented and probe-gated. It is `Available` only when a
  real Bubblewrap smoke test succeeds; otherwise it stays `Unavailable` and
  refuses. There is no silent downgrade to `project`.
- Strict L1 covers pure Marqdo and native Math; Foreign, native/worker
  plugins, kernels, external grants and trace files are rejected before the
  sandbox starts. Strict L2 inheritance is future work.
- Capability brokers and worker processes remain cooperative
  authorization/crash isolation, not an OS sandbox.
- The G-C lane was re-run at `8af7262` from a clean `git archive` in a
  disposable Ubuntu 26.04 VM as ordinary user `dev`: 7/7 adversarial tests,
  positive CLI contract and forged-marker rejection PASS. This is still a local
  VM result, not a self-hosted GitHub security-lane URL.

## Baseline `0.1.0` (before this stabilization slice)

- Markdown profile `marqdo.md.v1` with 150 AST goldens.
- Native Rust interpreter, analysis and execution conformance corpus.
- lock v3 package-instance graph with full transitive closure, verified Git
  cache and atomic `.marqdo/env` materialization (ADR-0016).
- Analysis v2 and Trace v2 with independent Python consumers and JSON Schemas.
- Tool Protocol v1 with SemVer/protocol negotiation and whole-bundle digests.
- Plugin ABI v1 native and worker transports with a brokered `fs.read`
  capability path (ADR-0018).
- Pinned Foreign provider/environment digests and one-shot Python/Node kernels.
