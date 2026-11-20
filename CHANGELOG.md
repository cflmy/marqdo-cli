# Changelog

All notable changes to the Marqdo CLI workspace. The workspace version is
currently `0.1.0`; no release tag has been cut. Dates use ISO 8601.

## Unreleased (stabilization candidate)

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
  code fences, and source spans inside one/two-backtick code spans.
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

- Assignment targets are no longer restricted to bare identifiers: a single
  index place is accepted at statement level. Member assignment stays a parse
  error; chained assignment stays a parse error. `spec/expression.ebnf`,
  `spec/evaluation.md`, `spec/value-wire.md`, `spec/analysis-v2.md` and
  `spec/security.md` document the new semantics.
- `docs/STABLE_RELEASE_IMPLEMENTATION.md` is the single next-phase
  implementation plan. Older plan documents are historical input, not current
  status.
- AST conformance tests fail early when a golden contains CR bytes instead of
  reporting an opaque mismatch.
- Worker plugin fixtures use the platform dynamic-library name and compiler
  flags.
- List-item condition paragraphs now accept only inert description; an
  executable `Strong`/`Emph` there is a hard `markdown.invalid_construct`
  error instead of being silently swallowed. Condition expression spans are
  mapped from the literal bytes inside the code-span backticks.
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
