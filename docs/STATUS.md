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

This checkout has a Gitee remote and **no GitHub credentials**. No new remote
CI URL can be produced from this host. The evidence below is local/VM evidence
for the convergence commit, not a substitute for the native GitHub lanes.

Convergence code/security SHA: **`8af72620f12e2494a59830a23abb393768b610e9`**
(the following docs commit only adds this report and release material; code is
byte-identical).

### Local static and test gate (Ubuntu 26.04, Rust 1.100.0)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  (47 test binaries, 0 failed; includes 7/7 Linux security tests)
```

### G-C security lane (disposable Ubuntu 26.04 VM, ordinary user `dev`)

Source: `git archive 8af7262` extracted in the VM (not the dev container).
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

Logs: [`evidence/2026-11-20-strict-gc-8af7262.md`](evidence/2026-11-20-strict-gc-8af7262.md).
This is still a **local VM lane**, not a self-hosted GitHub Actions security URL.

## 2. Work package status

| # | Work package | State | Evidence |
|---|---|---|---|
| 0 | Windows AST line endings | Fixed locally; **Windows CI UNVERIFIED** | `.gitattributes`, CRLF fixture |
| 1 | macOS native plugin fixtures | Fixed locally; **macOS CI UNVERIFIED** | host-target manifest + dylib flags |
| 2 | Cross-platform strict `cfg` | **Fixed locally; native CI UNVERIFIED** | `#[cfg]` dispatch + non-Linux regression test |
| 3 | Strict probe→launch race | **Fixed locally; GitHub Linux CI UNVERIFIED** | fail-closed CLI contract + probe-OK/launch-EAGAIN seam test |
| 4 | Parallel security matrix isolation | **Fixed** | unique paths + helper `OnceLock`; 7/7 in parallel workspace run |
| 5 | Markdown/power/control-flow corpus | **Done locally** | 160 AST, 70 semantic, 52 compile-negative, 21 runtime-negative fixtures |
| 6 | Condition header + source spans | **Fixed** | executable tails rejected; literal-inside-backtick mapping tests |
| 7 | Independent Tool consumers | **Done locally** | `sample-view`/`sample-knowledge`/`sample-proof` E2E test |
| 8 | lock v3 reproducibility | **Implemented; tamper test added** | `package_env` PASS |
| 9 | Analysis/Trace/Tool/ABI/Foreign | **Implemented; supported-matrix bounded** | protocol tests, plugin/worker/foreign tests |
| 10 | Linux strict L1 | **VM PASS at 8af7262; no CI URL** | 7/7 + positive CLI |
| 11 | Linux strict L2 | **NOT IMPLEMENTED / UNSUPPORTED** | strict rejects Worker/Foreign/native ABI |
| 12 | Windows Native/Worker ABI | **UNSUPPORTED** | no Windows ABI artifact/claim |
| 13 | RC artifact | **Linux candidate only; no release** | clean-install + checksums; macOS not built here |
| 14 | Release approval | **NOT STARTED** | requires owner approval |

## 3. Verified capabilities

- Frozen Markdown profile `marqdo.md.v1` with 160 AST goldens and explicit
  CRLF byte semantics.
- Native Rust interpreter with 70 semantic goldens, 52 compile-error fixtures,
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
- Compat-B print routing (ADR-0030): `Program.print_override` routes
  `打印/print` to the bundled `io.写值` when the program uses it; stdlib
  modules are injected lazily; resource operations are trace `effect` events
  and `HostOp::Print` no longer runs on that path. Strict permits only
  stdio/stdout; file/network resources remain refused.
- Full-value stdlib formatting (ADR-0029): FunctionRef `module/name` and
  PluginHandle `plugin/export` are read-only observation members (host id
  private); `stdlib/format.mq.md` reproduces builtin `打印`/`repr` for all 15
  value types, including Formula/Code/FunctionRef and Code UTF-8 byte length.
- Bundled standard library (CM-A10, ADR-0028): `stdlib/` ships
  `format`/`text`/`io`; explicit `std:` imports resolve only inside the bundle;
  a SHA-256 digest over `*.mq.md` is verified before loading; Analysis records
  `project.stdlib`; a frozen-binary test asserts the CLI hash is unchanged after
  running a `std:io` project and that tampering is rejected. `打印` is still
  the builtin default.
- Resource streams (RFC-CM CM-A06, ADR-0025): opaque host-issued `Resource`
  handles with `resource.open/read/write/close`; bytes move only through
  `Memory`; stdout write is capability/output-budget checked and project-file
  read/write is path/grant checked; file writes buffer until `close` and
  never flush on drop; a `.mq.md` chunked copy loop reproduces a file without a
  Rust `copy_file`; strict refuses every resource open until a provider is
  verified. Semantic fixture 66 and CLI/host tests.
- Minimal structure observation/construction (RFC-CM CM-A04, ADR-0023):
  value-semantics `list.append/concat`, `map.keys/has/get/set/remove`,
  Unicode scalar `text.codepoints`/`text.from_codepoint`, and non-negative
  `table.columns/row/cell`. Pure builtins; no mutation, no new syntax.

## 4. Open blockers and limits

1. **No remote CI evidence for the convergence SHA.** Windows/macOS fixes and
   the Linux EAGAIN contract are UNVERIFIED on GitHub. G1/G2/G3 remain FAIL
   until a real run exists.
2. **G-C is VM-verified but not CI-signed.** No self-hosted security-lane URL
   for 8af7262 exists.
3. Windows Native/Worker ABI is explicitly UNSUPPORTED (no Windows `cc`
   fixture lane, Unix-gated worker tests).
4. `fs.write`, `net.request`, `process`, `clock`, `random` are not executed
   broker services; unsupported requests must return `capability.unavailable`.
5. Resource streams are a **P3 first slice** only: project-file read and
   buffered write plus stdout, but no directories, network, process, device,
   generic provider or stdlib `print` migration; strict refuses every
   `resource.open` until the provider passes the isolation lane.
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
| P2 Marqdo-written allocator/data structures + minimal structure observation | **PARTIAL, gate NOT passed** | `.mq.md`-only evidence: bump allocator with u32 codec, linked list, failed-allocation atomicity (48–50); CM-A04 structure primitives and a table filter (51–54, ADR-0023); a first-fit free-list allocator with block reuse, splitting, address-adjacent coalescing and `Result` maps (55, ADR-0024), a `Memory`-backed binary tree with recursive insert/in-order traversal (56), a `.mq.md` CSV row splitter (57), an open-addressing hash table with linear probing, update and `hash.full` (58), a `.mq.md` UTF-8 encoder over `Memory` (59), a validating UTF-8 decoder rejecting truncated, bad-continuation and surrogate/overlong input (60), a `Memory`-backed graph adjacency list (61), a recursive JSON parser for nested arrays/objects, strings with escapes, integers, booleans, null and error Results (62), a `Memory`-backed stack bytecode VM with PUSH/ADD/MUL/JMP/JZ/HALT (63), big-endian 16/32/64-bit plus LEB128 integer codecs (64), a `.mq.md` table sort rebuilding a new table from `table.row/columns/cell` (65), a table join (67), a quoted/escaped CSV row parser (68), table group (69), and an Int matrix multiply (70); the first-fit allocator also detects double free (`alloc.double_free`). Evidence: [`memory-p2-closure`](evidence/2026-11-20-memory-p2-closure.md), [`structure-api-p2`](evidence/2026-11-20-structure-api-p2.md), [`result-and-free-list-p2`](evidence/2026-11-20-result-and-free-list-p2.md). P2 remaining: publication as a separate external stdlib repository. The bundled `stdlib/` tree, `std:` imports, digest verification, Analysis identity and frozen-CLI closure test are implemented (ADR-0028). C01–C06/C11/C12 each have `.mq.md` evidence, including JSON floats and a JSON formatter. |
| P3 Resource/Stream ABI | **PARTIAL** | ADR-0025/0026 CM-A06 slices: opaque `Resource`, `resource.open/read/write/close`, stdout write in pure/project, granted project-file read and buffered project-file write in project, EOF/closed/handle errors, wire/host rejection; strict permits only stdio/stdout and refuses file/network resources. Fixture 66 + CLI/host tests. CM-A07 **Compat-B** is implemented: when a program calls `打印/print`, the loader injects the bundled `stdlib/io.mq.md` and the runtime routes to `io.写值`, which formats all 15 value types, UTF-8-encodes via `Memory` and writes via `resource.write(stdout)`; legacy `HostOp::Print` is only a no-bundle fallback. Strict permits only the existing stdio console channel. Still missing: Compat-C, directories, network, generic provider registry. |
| P4 Table logical/physical migration | **NOT IMPLEMENTED** | `Value::Table` remains `Vec<Vec<Value>>`; no stdlib table algorithms |
| P5 third-party Provider / TCP / HTTP | **NOT IMPLEMENTED** | `fs.write`, `net.request`, `process`, `clock`, `random` remain unavailable broker services (see §4) |
| P6 performance / native backends | **NOT IMPLEMENTED** | no bytecode/JIT/GPU/QPU work |

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
