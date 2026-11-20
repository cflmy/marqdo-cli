# Marqdo Implementation Status

Snapshot: **2026-11-20**, workspace version `0.1.0`, no release tag.
Authoritative CI convergence plan:
[`NEXT_RELEASE_CI_CONVERGENCE.md`](NEXT_RELEASE_CI_CONVERGENCE.md).
Support matrix: [`../COMPATIBILITY.md`](../COMPATIBILITY.md).

This file records what was actually executed on the current tree. "Implemented"
does not mean "verified on every platform"; the evidence column says which
commands were run and where. The historical M0.5 snapshot is preserved at
[`history/STATUS-M0.5-snapshot.md`](history/STATUS-M0.5-snapshot.md) and must
not be read as current status.

## 1. Remote baseline and current evidence

GitHub `master` is still `c9f01aa8fb127a2dcc125103ef794832984774ad`; the latest
public CI is run
[44692692318](https://github.com/cflmy/marqdo-cli/actions/runs/44692692318)
and is **red**:

- Linux `fmt + clippy + test`: FAIL —
  `strict_mode_enforces_isolation_or_refuses_closed` hit
  `bwrap: Creating new namespace failed: Resource temporarily unavailable`.
- macOS / Windows `cargo test --workspace`: FAIL — `E0433` on
  `linux::probe()` under `cfg!`.
- Schema instance validation + consumers: PASS.
- Release candidate artifact: SKIPPED (upstream failure; tag/dispatch only).

This checkout's private remote is `https://git.cflmy.cn/cflmy/marqdo-cli` with
Woodpecker CI at `https://ci.cflmy.cn` (`.woodpecker.yml`). The Linux Docker
pipeline runs fmt/clippy/test/release-build. GitHub native lanes, Windows/macOS
runners and a CI-attached strict VM runner are still unavailable; the G-C
evidence below remains local VM evidence.

Convergence code/security SHA: **`8af72620f12e2494a59830a23abb393768b610e9`**
(the following docs commit only adds this report and release material; code is
byte-identical).

### Local static and test gate (Ubuntu 26.04, Rust 1.100.0)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  430 passed, 0 failed, 2 ignored; includes 7/7 Linux security tests
```

### G-C security lane (disposable Ubuntu 26.04 VM, ordinary user `dev`)

Source: `git archive 70d916d` extracted in the VM (not the dev container).
Host: QEMU/TCG, kernel `7.0.0-43-generic`, `systemd-detect-virt --container`
= `none`, bubblewrap 0.11.1 with `bwrap-userns-restrict`, uid 1000.

```text
./tests/security/run-lane.sh
  adversarial helper matrix: 7 passed, 0 failed, 0 ignored
  positive CLI contract: PASS
  external-execution surfaces refused: PASS
  forged active marker rejected: PASS
  G-C security lane: PASS
cargo test --workspace --locked (same archive): 47 ok, 0 failed
```

Logs: [`evidence/2026-11-20-strict-resource-cm-a11.log`](evidence/2026-11-20-strict-resource-cm-a11.log)
and [`evidence/2026-11-20-strict-gc-8af7262.md`](evidence/2026-11-20-strict-gc-8af7262.md).
This is still a **local VM lane**, not a self-hosted GitHub Actions security URL.

## 2. Work package status

| # | Work package | State | Evidence |
|---|---|---|---|
| 0 | Windows AST line endings | Fixed locally; **Windows CI UNVERIFIED** | `.gitattributes`, CRLF fixture |
| 1 | macOS native plugin fixtures | Fixed locally; **macOS CI UNVERIFIED** | host-target manifest + dylib flags |
| 2 | Cross-platform strict `cfg` | **Fixed locally; native CI UNVERIFIED** | `#[cfg]` dispatch + non-Linux regression test |
| 3 | Strict probe→launch race | **Fixed locally; GitHub Linux CI UNVERIFIED** | fail-closed CLI contract + probe-OK/launch-EAGAIN seam test |
| 4 | Parallel security matrix isolation | **Fixed** | unique paths + helper `OnceLock`; 7/7 in parallel workspace run |
| 5 | Markdown/power/control-flow corpus | **Done locally** | 160 AST, 73 semantic, 52 compile-negative, 21 runtime-negative fixtures |
| 6 | Condition header + source spans | **Fixed** | executable tails rejected; literal-inside-backtick mapping tests |
| 7 | Independent Tool consumers | **Done locally** | `sample-view`/`sample-knowledge`/`sample-proof` E2E test |
| 8 | lock v3 reproducibility | **Implemented; tamper test added** | `package_env` PASS |
| 9 | Analysis/Trace/Tool/ABI/Foreign | **Implemented; supported-matrix bounded** | protocol tests, plugin/worker/foreign tests |
| 10 | Linux strict L1 | **VM PASS at 70d916d; strict needs a dedicated CI runner** | 7/7 + positive CLI + strict resource refusal; general Linux Docker CI is green |
| 11 | Linux strict L2 | **NOT IMPLEMENTED / UNSUPPORTED** | strict rejects Worker/Foreign/native ABI |
| 12 | Windows Native/Worker ABI | **UNSUPPORTED** | no Windows ABI artifact/claim |
| 13 | RC artifact | **Linux candidate only; no release** | clean-install + checksums; macOS not built here |
| 14 | Release approval | **NOT STARTED** | requires owner approval |

## 3. Verified capabilities

- Frozen Markdown profile `marqdo.md.v1` with 160 AST goldens and explicit
  CRLF byte semantics.
- Native Rust interpreter with 73 semantic goldens, 52 compile-error fixtures,
  and runtime error source attribution.
- Native `**` power is pinned end-to-end: right associativity, unary-minus
  precedence, parentheses, float negative exponents, branch/loop conditions,
  Strong+CodeSpan payloads, and inert prose/code fences.
- lock v3 full transitive package-instance graph, verified Git cache, strict
  `sync --locked`, atomic `.marqdo/env` materialization; `run`/`check` never
  fetch or rewrite the lock. Locked path content tamper is rejected without
  rewriting the lock or environment.
- Analysis v2 (logical identity, resources, byte digest) and Trace v2 (module
  ownership, bounds, truncation, stale detection) with independent Python
  consumers and JSON Schema validation.
- Tool Protocol v1 with arbitrary command names (no CLI hard-coding),
  SemVer/protocol negotiation, whole-bundle digest verification, atomic
  install/remove, and three independent Analysis/Trace consumers.
- Plugin ABI v1: native + worker transports, capability broker executing
  `fs.read` through `ProjectFs`, per-invocation budgets, host-owned handle
  identity, crash/timeout isolation. Not an OS sandbox.
- Foreign kernels: one-shot Python/Node, typed wire, timeout/output caps,
  provider/environment digests, environment-bound provider, interpreter
  version verification.
- Linux strict L1: Bubblewrap namespace supervisor, runtime probe, fail-closed
  launch, child self-verification, 7/7 adversarial matrix.
- Addressable memory (RFC-CM P1, ADR-0022): opaque `Memory` values shared by
  handle identity, region isolation, zero-filled 64 KiB page growth, bounded
  byte read/write, `memory.new/size/grow`, `m[i] = byte`, run and per-region
  budgets, host/wire rejection, Analysis v2 target projection.
- Recoverable `Result` convention (RFC-CM CM-A05, ADR-0024): success is
  `{ok:true,value:...}`, failure is `{ok:false,code,message[,details]}` as an
  ordinary Map; expected conditions return it, invariant violations stay
  fatal. A `.mq.md` first-fit/free-list allocator reuses blocks, splits free
  blocks, coalesces address-adjacent blocks and returns `alloc.no_space`
  without aborting.
- CM-A07 Compat-A (ADR-0027): explicit `.mq.md` `io.写值` chain
  (`format.显示` → UTF-8 encode → `Memory` → `resource.write(stdout)`) matches
  builtin `打印` for the covered value set; builtin remains default and no
  alias is routed.
- Compat-C print completion (ADR-0031): `HostOp::Print`/`op_print` removed;
  the print builtin only routes to bundled `io.写值` or fails with
  `kernel.unavailable`; trace has resource effects only.
- Compat-B print routing (ADR-0030): `Program.print_override` routes
  `打印/print` to the bundled `io.写值` when the program uses it; stdlib
  modules are injected lazily; resource operations are trace `effect` events
  and `HostOp::Print` no longer runs on that path. Strict permits only
  stdio/stdout; stdin/stderr/file/network resources remain refused.
- Stdio streams and console productization (ADR-0040): `stdio/stdin` is
  read-only over one shared run cursor; `stdio/stdout`/`stdio/stderr` are
  write-only with separate host buffers and output-budget accounting; the CLI
  copies non-terminal process stdin at composition and embeds program stderr as a
  separate JSON string (or writes it to CLI stderr). `stdlib/io/stream.mq.md`
  checks short writes and close errors; `stdlib/io/console.mq.md` implements
  line input and print through the resource ABI; `stdlib/text/utf8.mq.md`
  provides a validating UTF-8 decoder.
- Full-value stdlib formatting (ADR-0029): FunctionRef `module/name` and
  PluginHandle `plugin/export` are read-only observation members (host id
  private); `stdlib/format.mq.md` reproduces builtin `打印`/`repr` for all 15
  value types, including Formula/Code/FunctionRef and Code UTF-8 byte length.
- C ABI provider worker binding (CM-A09, ADR-0038): the separate
  `marqdo-provider-worker` helper loads a trusted C ABI library and serves the
  framed-JSON transport; process isolation but not a sandbox.
- Provider package manifest v1 (CM-A09, ADR-0037): a local `marqdo.provider`
  manifest resolves a trusted C ABI library and registers it; no registry,
  signatures or CLI install yet.
- C ABI ResourceProvider v1 (CM-A09, ADR-0036): trusted shared libraries expose
  open/read/write/close through the frozen header; version/load failures are
  fail-closed; project-mode + name grant only. Plugin-Worker binding remains open.
- Worker resource provider transport (CM-A09, ADR-0035): host-managed helper
  process with framed JSON, ping/open/read/write/close; project-mode + name
  grant; trusted, not a sandbox; C ABI binding still open.
- HTTP framing and client (P5/C10): `stdlib/net/http.mq.md` parses status,
  headers and `Transfer-Encoding: chunked`, builds requests, and an end-to-end
  test parses a loopback TCP response; fixture 71 pins the same logic. No host
  HTTP/TLS implementation.
- Blocking TCP endpoint (P5 first slice, ADR-0034): project-mode
  `resource.open("tcp", ip:port, ["read","write"])` with exact IP grant and
  timeouts; no DNS/TLS/UDP/server sockets; strict/pure refuse. CLI `--allow-net`.
- In-process ResourceProvider v1 (CM-A09 first slice, ADR-0033): trusted
  providers register additional resource kinds without parser/runtime/HostOp
  changes; project-mode + name grant required; pure/strict refuse provider
  kinds; C/Worker transport remains open.
- P6 baseline profile: `tools/perf/bench.py` measures 7 `.mq.md` workloads with
  wall time and peak RSS; no optimizer/backend was added and P6 is not claimed
  complete.
- Cross-storage table equivalence (P4, ADR-0032): fixture 72 compares T0 row
  Table results with a `.mq.md` columnar List/Map store for sort/filter.
- Table stdlib algorithms (CM-A08, ADR-0032): `stdlib/data/table.mq.md`
  provides select/filter/sort/append/set/join/group/`转GFM` over the logical
  `Value::Table`; inputs are never mutated; GFM AST laziness and physical T0
  storage are unchanged.
- Bundled standard library (CM-A10, ADR-0028): `stdlib/` ships
  `format`/`text`/`io`; explicit `std:` imports resolve only inside the bundle;
  a SHA-256 digest over `*.mq.md` is verified before loading; Analysis records
  `project.stdlib`; a frozen-binary test asserts the CLI hash is unchanged after
  running a `std:io` project and that tampering is rejected. `打印/print` is
  routed to the bundled stdlib (Compat-C, ADR-0030/0031); there is no legacy
  host formatter. `tools/pack-stdlib.py` additionally emits a byte-deterministic
  offline archive + descriptor as an extraction-ready artifact
  ([evidence](evidence/2026-11-20-stdlib-package.md)). The stdlib intentionally
  stays in this repository for now (ADR-0039); extraction is a future option
  when size/release cadence requires it, not a pending gate.
- Resource streams (RFC-CM CM-A06, ADR-0025/0026/0040): opaque host-issued
  `Resource` handles with `resource.open/read/write/close`; bytes move only
  through `Memory`; stdin is read-only over one shared run cursor; stdout and
  stderr are separate capability/output-budget checked buffers; project-file
  read/write is path/grant checked; file writes buffer until `close` and
  never flush on drop; a `.mq.md` chunked copy loop reproduces a file without a
  Rust `copy_file`; strict permits only the stdio stdout console channel and
  refuses stdin/stderr/file/TCP/provider resources (CM-A11 lane PASS). Semantic
  fixture 66 and CLI/host tests.
- Minimal structure observation/construction (RFC-CM CM-A04, ADR-0023):
  value-semantics `list.append/concat`, `map.keys/has/get/set/remove`,
  Unicode scalar `text.codepoints`/`text.from_codepoint`, and non-negative
  `table.columns/row/cell`. Pure builtins; no mutation, no new syntax.

## 4. Open blockers and limits

1. **No native GitHub/Windows/macOS CI evidence.** The private Woodpecker
   Linux Docker pipeline (fmt/clippy/test/release-build) is green, but Windows
   and macOS native lanes are still unavailable. G1/G2/G3 remain FAIL until a
   real native run exists.
2. **G-C is VM-verified but not CI-signed.** The Woodpecker Docker pipeline is
   not an OS-isolation lane; no self-hosted security runner exists yet.
3. Windows Native/Worker ABI is explicitly UNSUPPORTED (no Windows `cc`
   fixture lane, Unix-gated worker tests).
4. `fs.write`, `net.request`, `process`, `clock`, `random` are not executed
   broker services; unsupported requests must return `capability.unavailable`.
5. Resource streams cover project-file read/write, stdout, compatible print,
   trusted in-process providers and IP-literal TCP; directories, process,
   device, DNS/TLS/HTTP, UDP and provider packaging remain open. Strict permits
   only stdio and refuses the rest (CM-A11 lane PASS).
6. uv/npm/venv provisioning is not implemented. Only a pre-provisioned,
   digest-pinned directory with a regular-file executable is supported.
7. No Windows/macOS RC archive exists from this host. The release-candidate
   workflow can build macOS on GitHub, but it has not run for this SHA.
8. No release tag/binary publication. The release decision is **DO NOT
   RELEASE** until the native CI matrix and a security-lane CI URL exist.

## 5. RFC-CM compute-machine status

The two 2026-11-20 architecture RFCs are design input, not implemented
behavior. This table records only what is in this tree now.

| RFC phase | State | Evidence / limit |
|---|---|---|
| P0 freeze (ADRs, spec deltas, baseline) | **P1 + partial P2 slice done** | ADR-0022 freezes CM-A01/CM-A02/CM-A03; ADR-0023 freezes CM-A04 (minimal List/Map/Text/Table observation and construction); ADR-0024 freezes CM-A05 (recoverable `Result` map convention); `spec/expression.ebnf`, `spec/evaluation.md`, `spec/value-wire.md`, `spec/analysis-v2.md`, `spec/security.md` updated; existing AST/value goldens unchanged |
| P1 Memory + place assignment | **Implemented locally** | `Value::Memory`, run-scoped `MemoryStore` (64 KiB pages, 64 MiB total / 16 MiB region), `memory.new/size/grow`, `m[i]` read, `m[i] = byte`; semantic fixtures 45–47, runtime negatives 13–18, `examples/memory-buffer.mq.md`; evidence [`evidence/2026-11-20-memory-p1.md`](evidence/2026-11-20-memory-p1.md) |
| P2 Marqdo-written allocator/data structures + minimal structure observation | **PASS (local pure lane); stdlib remains in-repo by design** | `.mq.md`-only evidence: bump allocator with u32 codec, linked list, failed-allocation atomicity (48–50); CM-A04 structure primitives and a table filter (51–54, ADR-0023); a first-fit free-list allocator with block reuse, splitting, address-adjacent coalescing and `Result` maps (55, ADR-0024), a `Memory`-backed binary tree with recursive insert/in-order traversal (56), a `.mq.md` CSV row splitter (57), an open-addressing hash table with linear probing, update and `hash.full` (58), a `.mq.md` UTF-8 encoder over `Memory` (59), a validating UTF-8 decoder rejecting truncated, bad-continuation and surrogate/overlong input (60), a `Memory`-backed graph adjacency list (61), a recursive JSON parser for nested arrays/objects, strings with escapes, integers, booleans, null and error Results (62), a `Memory`-backed stack bytecode VM with PUSH/ADD/MUL/JMP/JZ/HALT (63), big-endian 16/32/64-bit plus LEB128 integer codecs (64), a `.mq.md` table sort rebuilding a new table from `table.row/columns/cell` (65), a table join (67), a quoted/escaped CSV row parser (68), table group (69), an Int matrix multiply (70), and a sparse-matrix multiply (73); the first-fit allocator also detects double free (`alloc.double_free`). Evidence: [`memory-p2-closure`](evidence/2026-11-20-memory-p2-closure.md), [`structure-api-p2`](evidence/2026-11-20-structure-api-p2.md), [`result-and-free-list-p2`](evidence/2026-11-20-result-and-free-list-p2.md). Local gate PASS: `crates/cli/tests/p2_closure.rs` runs 21 C01–C06/C11/C12 fixtures plus the stdlib table self-test in `--mode pure` (no foreign/plugin/import grants), scans sources for foreign/plugin/import shortcuts and compares value/stdout to goldens; evidence [`p2-closure-gate`](evidence/2026-11-20-p2-closure-gate.md). The bundled `stdlib/` tree, `std:` imports, digest verification, Analysis identity and frozen-CLI closure test are implemented (ADR-0028). The stdlib intentionally remains in-repo (ADR-0039); a separate repository is a future extraction option, not a gate. Linux Docker CI is green; a strict security runner and Windows/macOS lanes are still pending. |
| P3 Resource/Stream ABI | **PARTIAL** | ADR-0025/0026/0040 CM-A06 slices: opaque `Resource`, `resource.open/read/write/close`, stdin read, separate stdout/stderr write, granted project-file read and buffered project-file write in project, EOF/closed/handle errors, wire/host rejection; strict permits only stdio/stdout and refuses stdin/stderr/file/network resources. `stdlib/io/stream.mq.md`, `stdlib/io/console.mq.md` and `stdlib/text/utf8.mq.md` are product-level `.mq.md` APIs; fixture 66 + CLI/host tests. CM-A07 **Compat-C** is implemented: when a program calls `打印/print`, the loader injects the bundled `stdlib/io.mq.md` and the runtime routes to `io.写值` (all 15 value types, UTF-8 via `Memory`, `resource.write(stdout)`); `HostOp::Print` and the legacy `Value::display()` print path are removed, and a missing bundle fails with `kernel.unavailable`. Strict permits only the existing stdio console channel. Providers are registered through ADR-0033 (in-process) and ADR-0035 (worker helper). Still missing: directories, C ABI provider binding, provider packages, strict provider support. |
| P4 Table logical/physical migration | **PASS (local gate); T1/T2/T3 future** | ADR-0032/CM-A08: `Value::Table` semantics and GFM AST laziness unchanged; `stdlib/data/table.mq.md` provides select/filter/sort/append/set/join/group/`转GFM` as value-returning `.mq.md` functions with input immutability; zero Rust table algorithms were added. Cross-storage equivalence is pinned by fixture 72 (row Table vs `.mq.md` columnar store, identical headers/rows/cells). The optional T1/T2/T3 physical-storage/column-store migration was not required by the gate and is not implemented. |
| P5 third-party Provider / TCP / HTTP | **PASS (local gate); external network/CI and plugin-Worker resource binding open** | C10: `stdlib/net/http.mq.md` + fixture 71 parse HTTP/1.1 status/headers/chunked bodies and build requests; `http_over_tcp.rs` runs a loopback HTTP exchange wholly in `.mq.md` over `resource.open("tcp",...)`. ADR-0034 IP-literal blocking TCP endpoint (exact IP grant, project-only, 5s timeouts) and ADR-0033 in-process `ResourceProvider` v1 (registration, protocol/kind checks, name grant, mock read/write provider) ADR-0035 host-managed worker provider transport (framed JSON helper, ping/open/read/write/close, trusted-only) and ADR-0036 trusted C ABI provider (shared-library symbols, version/load checks) ADR-0037 local provider-package manifest installation and ADR-0038 C ABI provider worker binding (separate trusted helper process) are tested. External DNS/TLS/UDP, host HTTP, provider packages/signatures and the plugin-Worker resource binding remain out of scope. |
| P6 performance / native backends | **BASELINE ONLY** | reproducible profile harness `tools/perf/bench.py` and [`p6-profile`](evidence/2026-11-20-p6-profile.md) for 7 workloads; no bytecode/JIT/columnar numeric backend or equivalence/optimization gate yet |

The P1 memory domain is pure computation: it needs no grant and is available in
`pure`, `project` and `strict` (bounded by the memory budgets). It adds no
`HostOp`, no Markdown role and no CLI command. `Memory` handles cannot cross
the wire or a host/plugin/foreign boundary. Block-level safety of a
user-written allocator is explicitly **not** claimed.

## 6. Evidence files

- [`evidence/2026-11-20-local-gate.md`](evidence/2026-11-20-local-gate.md) —
  earlier local gate and Windows autocrlf reproduction.
- [`evidence/2026-11-20-strict-gc-8af7262.md`](evidence/2026-11-20-strict-gc-8af7262.md)
  — current VM lane, environment, full log and workspace test log.
- [`evidence/2026-11-20-resource-stream-p3.md`](evidence/2026-11-20-resource-stream-p3.md)
  — CM-A06 first slice: stdout write, project-file chunked read, strict refusal,
  wire/host boundary rejection and test commands.
- [`evidence/2026-11-20-resource-file-write-p3.md`](evidence/2026-11-20-resource-file-write-p3.md)
  — buffered project-file write, flush-on-close, buffer ceiling, grants and
  CLI write-then-read evidence.
- [`evidence/2026-11-20-stdlib-print-compat-a.md`](evidence/2026-11-20-stdlib-print-compat-a.md)
  — CM-A07 Compat-A `.mq.md` print prototype, side-by-side output comparison,
  covered/uncovered value types and the Compat-B blocker.
- [`evidence/2026-11-20-bundled-stdlib-cm-a10.md`](evidence/2026-11-20-bundled-stdlib-cm-a10.md)
  — CM-A10 bundled stdlib, `std:` imports, digest verification, Analysis
  identity and frozen-binary closure test.
- [`evidence/2026-11-20-full-value-formatting.md`](evidence/2026-11-20-full-value-formatting.md)
  — ADR-0029 observation members and all-15-type stdlib formatting comparison.
- [`evidence/2026-11-20-print-compat-b.md`](evidence/2026-11-20-print-compat-b.md)
  — ADR-0030 Compat-B print routing, trace resource effects, lazy stdlib
  injection and strict stdio console permission.
- [`evidence/2026-11-20-print-compat-c.md`](evidence/2026-11-20-print-compat-c.md)
  — ADR-0031 removal of `HostOp::Print`, no-bundle fail-closed behavior and
  embedder compatibility note.
- [`evidence/2026-11-20-table-cm-a08.md`](evidence/2026-11-20-table-cm-a08.md)
  — ADR-0032 table logical value, stdlib select/filter/sort/append/set/join/
  group, input immutability and T0 storage boundary.
- [`evidence/2026-11-20-provider-abi-cm-a09.md`](evidence/2026-11-20-provider-abi-cm-a09.md)
  — ADR-0033 in-process ResourceProvider v1, grants/mode checks, mock provider
  read/write and registration rejection cases.
- [`evidence/2026-11-20-tcp-endpoint.md`](evidence/2026-11-20-tcp-endpoint.md)
  — ADR-0034 blocking TCP endpoint, IP grants, DNS refusal, loopback host/CLI
  echo tests.
- [`evidence/2026-11-20-table-cross-storage.md`](evidence/2026-11-20-table-cross-storage.md)
  — P4 row-store Table vs `.mq.md` columnar store equivalence for sort/filter.
- [`evidence/2026-11-20-c-worker-binding.md`](evidence/2026-11-20-c-worker-binding.md)
  — ADR-0038 C ABI provider worker binding, compiled C adapter in the helper
  process and fail-closed missing-library case.
- [`evidence/2026-11-20-provider-package.md`](evidence/2026-11-20-provider-package.md)
  — ADR-0037 local provider package manifest, C library resolution and
  registration test.
- [`evidence/2026-11-20-c-provider.md`](evidence/2026-11-20-c-provider.md)
  — ADR-0036 C ABI ResourceProvider v1, compiled C adapter round-trip and
  fail-closed load/version checks.
- [`evidence/2026-11-20-worker-provider.md`](evidence/2026-11-20-worker-provider.md)
  — ADR-0035 trusted worker resource-provider transport, framed-JSON helper
  round-trip and grant/mode/error tests.
- [`evidence/2026-11-20-http-over-tcp.md`](evidence/2026-11-20-http-over-tcp.md)
  — P5/C10 `.mq.md` HTTP framing over a loopback TCP stream, stdlib module and
  end-to-end client test.
- [`evidence/2026-11-20-p2-closure-gate.md`](evidence/2026-11-20-p2-closure-gate.md)
  — P2 C01–C06/C11/C12 pure no-Foreign/no-Plugin closure lane and golden
  comparison.
- [`evidence/2026-11-20-http-parser.md`](evidence/2026-11-20-http-parser.md)
  — P5/C10 `.mq.md` HTTP/1.1 status/header/chunked response parser and request
  builder fixture.
- [`evidence/2026-11-20-strict-resource-cm-a11.md`](evidence/2026-11-20-strict-resource-cm-a11.md)
  — CM-A11 strict lane at `00bdcad`: 7/7 adversarial matrix, positive Compat-C
  print, strict file-resource refusal, forged-marker rejection and raw log.
- [`evidence/RELEASE_CANDIDATE_0.1.0_8af7262.md`](evidence/RELEASE_CANDIDATE_0.1.0_8af7262.md)
  — G0..G12 matrix and RC decision.
- [`security-gate.md`](security-gate.md) — mechanism, attack matrix and strict
  L1/L2 limits.

## 7. Reproduce

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# On hosts with a read-only $HOME, point CARGO_HOME at a writable directory.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli
```

Consumer evidence:

```bash
./target/debug/marqdo check examples/multi-module/index.mq.md --refs --emit /tmp/a.json
./target/debug/marqdo run examples/multi-module/index.mq.md --trace-file /tmp/t.json
python3 tests/consumers/analysis_consumer.py /tmp/a.json
python3 tests/consumers/trace_consumer.py /tmp/a.json /tmp/t.json
```

G-C (ordinary user, disposable VM only):

```bash
MARQDO_REQUIRE_STRICT_SECURITY=1 ./tests/security/run-lane.sh
```
