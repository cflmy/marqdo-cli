# Marqdo Implementation Status

Snapshot: **2026-11-21**, workspace version `0.1.0`, no release tag.
Current architecture overview:
[`architecture/CURRENT.md`](architecture/CURRENT.md).
Authoritative CI convergence plan:
[`NEXT_RELEASE_CI_CONVERGENCE.md`](NEXT_RELEASE_CI_CONVERGENCE.md) (historical
baseline at `c9f01aa8`; refresh before use).
Support matrix: [`../COMPATIBILITY.md`](../COMPATIBILITY.md).

This file records what was actually executed on the current tree. "Implemented"
does not mean "verified on every platform"; the evidence column says which
commands were run and where. Do not copy a running status into another plan;
[`architecture/CURRENT.md`](architecture/CURRENT.md) is the single current
overview. The historical M0.5 snapshot is preserved at
[`history/STATUS-M0.5-snapshot.md`](history/STATUS-M0.5-snapshot.md) and must
not be read as current status.

Status is generated from executed commands and logs; a gate row without a command,
log or source path is a defect, not a state.

## 0. Language migration completed (RFC-LANG-0002)

The repository now has **one** language contract, implemented end to end:
[`architecture/RFC-LANG-0002-unified-semantics.md`](architecture/RFC-LANG-0002-unified-semantics.md)
(accepted by [ADR-0052](adr/0052-unified-markdown-semantics.md)), with the
normative summary in [`../spec/mapping.md`](../spec/mapping.md) and
[`../spec/expression.ebnf`](../spec/expression.ebnf).

Landed on `master` as one linear series (the former `semantics-v2-refactor`
branch was fast-forwarded and deleted; only `master` remains):

| Step | Commit | Content |
|---|---|---|
| 1 | `4983542` | docs/spec/ADR/READMEs, historical banners, migration guide and inventory |
| 2 | `e029853` | frontend/IR/runtime implementation, `sem.*` diagnostics, migrated stdlib + digest, examples, fixtures and Rust-embedded sources |
| 3 | `ffd1cc9` | expression-owned payload statement grammar, extra negative/CLI coverage |
| 4 | `bbeb1de` | status/compatibility/README/migration docs flipped to the implemented state + green evidence |
| 5 | `b38b769` | function-wide `params(...)` position (across heading segments) + two negatives |
| 6 | `361a6cc` | removed the dead `Stmt::Call`/`CallSite`/`CallTarget` IR |
| 7 | `d5002ac` | stale pre-RFC role docs and review-aid fixes |
| 8 | `9fb97bc` | signatures recognized only from Exec payloads (Knowledge sections inert) |
| 9 | `a3f9770` | unmatched `end` diagnostics point at the `end` payload |
| 10 | `0bbe0f6` | Setext headings never declare a mode; dead helper removed |
| 11 | `b3429cb` | diagnostics sorted by source position then code |
| 12 | `9c1723d` | coverage for the removed syntax-selector front-matter probe |

The breaking behaviour is:

* H1–H6 headings declare `@exec`/`@knowledge`; unmarked headings inherit the
  nearest ancestor and the document root is Knowledge;
* only an Exec H2 is callable; an H2 Knowledge unit and H3–H6 sections are not
  functions, and a payload with no owning Exec H2 is `sem.exec_without_function`;
* `Strong`, `Emph` and inline `Code` are interchangeable one-statement carriers;
  nested carriers and empty payloads are hard errors;
* `if`/`else`/`while`/`end`/`return`/`params` come from the payload and lower to
  the existing `Branch`/`Loop`/`Return`/`Assign`/`Eval` IR; lists, block quotes
  and paragraphs are presentation containers;
* control frames cannot cross a heading segment;
* `params(...)` is legal only as the first direct Exec H2 payload across the
  whole function (a later `### @exec` segment cannot add a signature after
  earlier code ran); without it the existing free-variable inference applies and
  inferred signatures keep the named-arguments-only rule;
* old syntax (`Strong` execute / `Emph` return / list branch/loop / quote call)
  is **not** executable, and `marqdo_semantics`/`syntax` front-matter selectors
  are rejected with `sem.unsupported_version_directive`.

### Verification at `9c1723d` (Linux x86_64, Rust 1.100.0)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked --no-fail-fast         PASS
  507 passed, 0 failed, 2 ignored (62 green test targets)
python3 tools/pack-stdlib.py                           PASS (digest verified)
```

Full evidence: [`evidence/2026-11-21-semantics-v2-green.md`](evidence/2026-11-21-semantics-v2-green.md).
The intermediate "text migrated, implementation pending" record is kept as
[`evidence/2026-11-21-semantic-docs-migration.md`](evidence/2026-11-21-semantic-docs-migration.md)
and is superseded by the green gate.

**Not claimed:** the **remote** G3 security artifact and a **MEASURED** G4
performance report are still missing. The Linux/Windows/macOS native matrix is
green at `5fcc111`, and the G3 mechanism is PASS in a local disposable VM, but
the RFC does not promote a local VM run to a remote gate. See §0.1, §4 and
[`../COMPATIBILITY.md`](../COMPATIBILITY.md). The release decision remains
**DO NOT RELEASE**.

## 0.1 CI/security convergence (2026-11-21)

The RFC-CI-0001 series fixes the release-blocking CI defects that the
`32d8fe1` GitHub run exposed. Commits on `master`:

| Step | Commit | Content |
|---|---|---|
| 1 | `f4a404b` | platform-neutral `StrictChildState`/`inspect_strict_child`; macOS/Windows no longer reference Linux-only functions (`E0425`) |
| 2 | `f817cce` | strict capture/routing: one JSON document on every `--json` path, stable failure codes, ADR-0053, deterministic routing tests |
| 3 | `c9f509e` | architecture guard against direct marker access; Windows cross-target check |
| 4 | `990e984` | G-C lane writes SHA-bound `strict-evidence.json`; workflow binds checkout/archive to the tested SHA and uploads evidence |
| 5 | `d2267e9` | correctness-verified `marqdo.perf.v2` sampler and fixed-node workflow |
| 6 | `0d7e11b` | SHA-bound release-evidence aggregator, docs drift cleanup, consistent `windows-latest`/`macos-latest` statement |
| 7 | `512b9f5` | evidence-generation failure fails the lane; workflow archive/verification fixes; G-C re-run |
| 8 | `beb68a9` | G-C evidence/status update and the cross-platform JSON smoke exit-code fix |
| 9 | `3a29fce` | Windows PATH lookup applies `PATHEXT`; ANSI-stripped native failure annotations |
| 10 | `b898c41` | macOS parser-nesting test stack; portable process product fixture (CLI `--version` instead of `/bin/echo`) |
| 11 | `4265231` | CLI runs on an explicit 8 MiB thread (Windows 1 MiB overflow); macOS temp-path canonicalization; G3 adversarial tests opt-in |
| 12 | `a24161c` | Windows foreign temp paths stay non-verbatim (Node `lstat` rejection) |
| 13 | `5fcc111` | reject rooted Windows dependency paths (`has_root`, not `is_absolute`) |

Executed evidence:

| Lane | Result | Evidence |
|---|---|---|
| Linux fmt/clippy/test/build (Woodpecker) | **PASS** | pipelines `#9`–`#12`, `#14`, `#16`, `#17`, `#21`–`#25` on the series; `#13`/`#15`/`#19` were superseded/cancelled by the next push, not failures |
| Linux + Windows + macOS native matrix (GitHub) | **PASS** | [run 30](https://github.com/cflmy/marqdo-cli/actions/runs/44896671929) on `5fcc111`: `fmt + clippy + test (Linux x86_64)`, `runtime contract (windows-latest)`, `runtime contract (macos-latest)`, and schema consumers all PASS |
| G3 strict isolation | **PASS (local disposable VM) / remote UNVERIFIED** | `5fcc111`, `dev` uid 1000, QEMU/TCG, 7/7 + strict JSON/inert-`@knowledge`; [`evidence/2026-11-21-strict-gc-5fcc111.md`](evidence/2026-11-21-strict-gc-5fcc111.md) |
| G4 performance | **SMOKE / UNVERIFIED** | nine workloads `VERIFIED` on the dev container, 1 warmup + 3 samples; [`evidence/2026-11-21-perf-smoke-0d7e11b.md`](evidence/2026-11-21-perf-smoke-0d7e11b.md) |

Release decision: **DO NOT RELEASE.** Linux, Windows and macOS native jobs are
green on the same SHA, and the strict isolation mechanism PASSes locally as a
normal user. The remaining release gates are the **remote** G3 artifact whose
`tested_sha` matches and a **MEASURED** G4 report on the fixed node; neither
remote claim is made here. The RFC explicitly requires the remote G3 artifact
for a release, so the local VM PASS is recorded but not promoted to a release
gate.

The manifest contract is in
[`evidence/ci/README.md`](evidence/ci/README.md); `tools/ci/release_evidence.py`
reports `DO_NOT_RELEASE` for any missing/failed/SHA-mismatched gate.

## 1. Remote baseline and current evidence

The local language baseline for this snapshot is `9c1723d` (2026-11-21,
semantics v2 implementation plus follow-up fixes). The later
documentation and test migration is recorded by its own commits; re-run the gate
before making a release claim.

The old GitHub `master` `c9f01aa8fb127a2dcc125103ef794832984774ad`
and public CI [run 44692692318](https://github.com/cflmy/marqdo-cli/actions/runs/44692692318)
are **historical (2026-11-20), not current**, and that run was red:

- Linux `fmt + clippy + test`: FAIL —
  `strict_mode_enforces_isolation_or_refuses_closed` hit
  `bwrap: Creating new namespace failed: Resource temporarily unavailable`.
- macOS / Windows `cargo test --workspace`: FAIL — `E0433` on
  `linux::probe()` under `cfg!`.
- Schema instance validation + consumers: PASS.
- Release candidate artifact: SKIPPED (upstream failure; tag/dispatch only).

The remote SHA must be refreshed from CI after a push; do not read `c9f01aa8`
as the current tree.

This checkout's private remote is `https://git.cflmy.cn/cflmy/marqdo-cli` with
Woodpecker CI at `https://ci.cflmy.cn` (`.woodpecker.yml`). The Linux Docker
pipeline runs fmt/clippy/test/release-build; the docs-consistency gate runs in
`cargo test --workspace`. The GitHub-hosted native matrix is now green at
`5fcc111` ([run 30](https://github.com/cflmy/marqdo-cli/actions/runs/44896671929)).
A CI-attached strict VM runner is still unavailable; the G-C evidence below
remains local VM evidence.

Convergence code/security SHA: **`8af72620f12e2494a59830a23abb393768b610e9`**
(the following docs commit only adds this report and release material; code is
byte-identical).

### Local static and test gate (Ubuntu 26.04, Rust 1.100.0)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  491 passed, 0 failed, 2 ignored; includes 7/7 Linux security tests
```

The documentation and test migration is local work; re-run this block after it
lands and update the count from the log rather than copying the number.
`docs_consistency`, `architecture_boundaries` and `stdlib_product` run inside
this workspace suite.

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

> **Historical table note.** Most rows in this section record work executed
> before the RFC-LANG-0002 syntax migration. Language-source rows (fixtures,
> control mapping, semantic goldens) were re-verified after the rewrite at
> `9c1723d` (§0); platform/security rows keep their original evidence dates and
> are not refreshed by the language migration.

| # | Work package | State | Evidence |
|---|---|---|---|
| 0 | Windows AST line endings | **Windows CI PASS** (`5fcc111`) | `.gitattributes`, CRLF fixture |
| 1 | macOS native plugin fixtures | **macOS CI PASS** (`5fcc111`) | host-target manifest + dylib flags |
| 2 | Cross-platform strict `cfg` | **Native CI PASS** (`5fcc111`) | `#[cfg]` dispatch + non-Linux regression test |
| 3 | Strict probe→launch race | **Linux CI PASS** (`5fcc111`) + deterministic seam | fail-closed CLI contract + probe-OK/launch-EAGAIN seam test |
| 4 | Parallel security matrix isolation | **Fixed** | unique paths + helper `OnceLock`; 7/7 in parallel workspace run |
| 5 | Markdown/power/control-flow corpus | **Green on semantics v2** | 160 AST + 73 semantic + 49 compile-negative + 2 warning + 21 runtime-negative fixtures; `conformance_ast`/`semantics`/`negative` pass |
| 6 | Payload/control source spans | **Green on semantics v2** | UTF-8 spans, control-expression offsets and heading boundaries covered by fixtures and CLI tests; old condition headers were removed |
| 7 | Independent Tool consumers | **Done locally** | `sample-view`/`sample-knowledge`/`sample-proof` E2E test |
| 8 | lock v3 reproducibility | **Implemented; tamper test added** | `package_env` PASS |
| 9 | Analysis/Trace/Tool/ABI/Foreign | **Implemented; supported-matrix bounded** | protocol tests, plugin/worker/foreign tests |
| 10 | Linux strict L1 | **Local VM PASS at 5fcc111; remote G3 runner still missing** | 7/7 + positive CLI + strict JSON/inert-`@knowledge`; [`strict-gc-5fcc111`](evidence/2026-11-21-strict-gc-5fcc111.md) |
| 11 | Linux strict L2 | **NOT IMPLEMENTED / UNSUPPORTED** | strict rejects Worker/Foreign/native ABI |
| 12 | Windows Native/Worker ABI | **UNSUPPORTED** | no Windows ABI artifact/claim |
| 13 | RC artifact | **Linux candidate only; no release** | clean-install + checksums; macOS not built here |
| 14 | Release approval | **NOT STARTED** | requires owner approval |

## 3. Verified capabilities

- Frozen Markdown profile `marqdo.md.v1` with 160 AST goldens and explicit
  CRLF byte semantics (unchanged by the semantic rewrite).
- Native Rust interpreter with the unified semantics: 73 semantic fixtures,
  49 compile-error fixtures, 2 warning fixtures and 21 runtime-error fixtures
  all green at `9c1723d`; AST profile unchanged (160 goldens).
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
  storage are unchanged. `std:fs/file` provides bounded `读文本`/`写文本`/`复制文件`
  over the existing `resource.*` ABI plus `存在`/`元数据`/`列出目录`/`创建目录`/
  `删除`/`重命名`/`追加文本`/`读取片段`/`遍历` over `resource.request`
  (ADR-0041/0045); `data/json`/`data/csv`
  provide recursive JSON (128-depth bound, multi-value) and quoted single/multi-row
  CSV product parsers; `path` and a `regex` subset are pure `.mq.md`; `datetime` is pure UTC ISO-8601 and
  `clock.现在` reads `clock.now` in project mode (ADR-0042); `env.读取` reads
  only exact `--allow-env NAME` grants in project mode (ADR-0043); `process.运行`
  runs an exact `--allow-process PROGRAM` grant with 5 s timeout and 1 MiB caps
  (ADR-0044, not an OS sandbox); `random.字节` reads explicitly granted
  `random.bytes` entropy (ADR-0046). File watch, binary offset reads, streaming
  incremental JSON/CSV streaming, pure-mode regex groups/alternation and regex
  look-around/backreferences remain open; project mode has host-backed groups
  and captures (ADR-0050).
- Bundled standard library (CM-A10, ADR-0028): `stdlib/` ships
  `format`/`text`/`io`/`fs`/`data`/`path`/`regex`/`datetime`/`clock`/`env`/`process`/`random`/`compress`/`net/http`; explicit `std:` imports resolve only inside the bundle;
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
  Rust `copy_file`; `resource.request` adds non-streaming `fs.exists`/
  `fs.metadata`/`fs.list`/`fs.mkdir`/`fs.remove`/`fs.rename` (project mode,
  grant-checked, recoverable fs Result); strict permits only the stdio stdout
  console channel and refuses stdin/stderr/file/TCP/provider resources (CM-A11
  lane PASS). Semantic fixture 66 and CLI/host tests.
- Test and documentation migration (review RFC-CM P0/P2):
  `crates/cli/tests/docs_consistency.rs` pins one current overview, the
  status/index links and SHA-pinned evidence; `crates/cli/tests/architecture_boundaries.rs`
  pins the workspace graph, `marqdo-runtime -> marqdo-host`, the bundled stdlib
  digest and the stdio/file/TCP endpoints; `crates/cli/tests/stdlib_product.rs`
  runs every public stdlib module (`clock/compress/console/csv/datetime/demo/env/format/fs/http/io/json/path/process/random/regex/stream/table/text/utf8`)
  through `std:` imports over `stdlib/tests/*.mq.md`. `tools/pack-stdlib.py`
  now sorts `.mq.md` by the same relative path key as the CLI. Source/test baseline
  `0d40de7`; later docs-only commits keep code/test bytes identical. Evidence:
  [`docs-and-test-migration`](evidence/2026-11-21-docs-and-test-migration.md).
- Module-boundary refactor (review RFC-CM P1): CLI split into
  `commands/{sandbox,run,check,inspect,init,package,tool}.rs`; host split into
  `host/{resource_ops,providers,effects,dispatch}.rs` plus `process.rs`.
  `commands.rs` keeps shared project/runtime helpers and `host.rs` keeps the
  `Host` trait methods and capability checks; this is a behavior-preserving
  split, not a new crate or backend.
- Minimal structure observation/construction (RFC-CM CM-A04, ADR-0023):
  value-semantics `list.append/concat`, `map.keys/has/get/set/remove`,
  Unicode scalar `text.codepoints`/`text.from_codepoint`, and non-negative
  `table.columns/row/cell`. Pure builtins; no mutation, no new syntax.

## 4. Open blockers and limits

1. **Native matrix is green; remote G3/G4 are not.** GitHub run 30 on
   `5fcc111` PASSes Linux, Windows and macOS, and Woodpecker #25 is green. The
   remaining release blockers are a remote G3 security artifact and a
   `MEASURED` G4 report on the fixed node.
2. **G-C is VM-verified but not CI-signed.** The Woodpecker Docker pipeline is
   not an OS-isolation lane; no ephemeral self-hosted security runner has run
   `security-linux.yml` for this SHA.
3. Windows Native/Worker ABI is explicitly UNSUPPORTED (no Windows `cc`
   fixture lane, Unix-gated worker tests).
4. `fs.write`, `net.request`, `process`, `clock`, `random` are not executed
   broker services; unsupported requests must return `capability.unavailable`.
5. Resource streams cover project-file read/write (including `std:fs/file`
   read/write/copy/append/read_at/walk), non-streaming `resource.request`
   `fs.*` metadata/directory operations, stdout, compatible print, trusted
   in-process providers and TCP/UDP/listener/TLS endpoints with DNS
   resolution after an exact host grant. File watch, device and provider
   packaging remain open. Strict permits only stdio and refuses the rest (CM-A11 lane
   PASS).
6. uv/npm/venv provisioning is not implemented. Only a pre-provisioned,
   digest-pinned directory with a regular-file executable is supported.
7. No tagged Windows/macOS RC archive exists. The native **build/test** lanes
   PASS at `5fcc111`; the release-candidate job is tag/dispatch gated and has
   not produced those archives.
8. No release tag/binary publication. The release decision is **DO NOT
   RELEASE** until the remote G3 artifact and the MEASURED G4 report exist.

## 5. RFC-CM compute-machine status

The two 2026-11-20 architecture RFCs are design input, not implemented
behavior. This table records only what is in this tree now.

| RFC phase | State | Evidence / limit |
|---|---|---|
| P0 freeze (ADRs, spec deltas, baseline) | **P1 + partial P2 slice done** | ADR-0022 freezes CM-A01/CM-A02/CM-A03; ADR-0023 freezes CM-A04 (minimal List/Map/Text/Table observation and construction); ADR-0024 freezes CM-A05 (recoverable `Result` map convention); `spec/expression.ebnf`, `spec/evaluation.md`, `spec/value-wire.md`, `spec/analysis-v2.md`, `spec/security.md` updated; existing AST/value goldens unchanged |
| P1 Memory + place assignment | **Implemented locally** | `Value::Memory`, run-scoped `MemoryStore` (64 KiB pages, 64 MiB total / 16 MiB region), `memory.new/size/grow`, `m[i]` read, `m[i] = byte`; semantic fixtures 45–47, runtime negatives 13–18, `examples/memory-buffer.mq.md`; evidence [`evidence/2026-11-20-memory-p1.md`](evidence/2026-11-20-memory-p1.md) |
| P2 Marqdo-written allocator/data structures + minimal structure observation | **PASS (local pure lane); stdlib remains in-repo by design** | `.mq.md`-only evidence: bump allocator with u32 codec, linked list, failed-allocation atomicity (48–50); CM-A04 structure primitives and a table filter (51–54, ADR-0023); a first-fit free-list allocator with block reuse, splitting, address-adjacent coalescing and `Result` maps (55, ADR-0024), a `Memory`-backed binary tree with recursive insert/in-order traversal (56), a `.mq.md` CSV row splitter (57), an open-addressing hash table with linear probing, update and `hash.full` (58), a `.mq.md` UTF-8 encoder over `Memory` (59), a validating UTF-8 decoder rejecting truncated, bad-continuation and surrogate/overlong input (60), a `Memory`-backed graph adjacency list (61), a recursive JSON parser for nested arrays/objects, strings with escapes, integers, booleans, null and error Results (62), a `Memory`-backed stack bytecode VM with PUSH/ADD/MUL/JMP/JZ/HALT (63), big-endian 16/32/64-bit plus LEB128 integer codecs (64), a `.mq.md` table sort rebuilding a new table from `table.row/columns/cell` (65), a table join (67), a quoted/escaped CSV row parser (68), table group (69), an Int matrix multiply (70), and a sparse-matrix multiply (73); the first-fit allocator also detects double free (`alloc.double_free`). Evidence: [`memory-p2-closure`](evidence/2026-11-20-memory-p2-closure.md), [`structure-api-p2`](evidence/2026-11-20-structure-api-p2.md), [`result-and-free-list-p2`](evidence/2026-11-20-result-and-free-list-p2.md). Local gate PASS: `crates/cli/tests/p2_closure.rs` runs 21 C01–C06/C11/C12 fixtures plus the stdlib table self-test in `--mode pure` (no foreign/plugin/import grants), scans sources for foreign/plugin/import shortcuts and compares value/stdout to goldens; evidence [`p2-closure-gate`](evidence/2026-11-20-p2-closure-gate.md). The bundled `stdlib/` tree, `std:` imports, digest verification, Analysis identity and frozen-CLI closure test are implemented (ADR-0028). The stdlib intentionally remains in-repo (ADR-0039); a separate repository is a future extraction option, not a gate. Linux/Windows/macOS native CI is green at `5fcc111`; a remote G3 security runner and a fixed G4 performance node are still pending. |
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
- [`evidence/2026-11-21-strict-gc-5fcc111.md`](evidence/2026-11-21-strict-gc-5fcc111.md)
  — RFC-CI-0001 series G-C: local disposable VM, `dev` uid 1000, 7/7 plus the
  strict JSON and inert-`@knowledge` contracts at `5fcc111`; the earlier `512b9f5`/`0d7e11b` runs are historical.
- [`evidence/2026-11-21-perf-smoke-0d7e11b.md`](evidence/2026-11-21-perf-smoke-0d7e11b.md)
  — `marqdo.perf.v2` smoke: nine workloads correctness-verified, explicitly
  not a G4 release baseline.
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

G-C (ordinary user, disposable VM only; bind the source archive and write the
evidence document):

```bash
export MARQDO_TESTED_SHA="$(git rev-parse HEAD)"
export MARQDO_SOURCE_ARCHIVE="/path/to/marqdo-${MARQDO_TESTED_SHA}.tar"
export MARQDO_SOURCE_ARCHIVE_SHA256="sha256:<host-computed digest>"
export MARQDO_STRICT_EVIDENCE="/tmp/evidence-${MARQDO_TESTED_SHA}/strict-evidence.json"
export MARQDO_REQUIRE_STRICT_SECURITY=1
./tests/security/run-lane.sh
```

A root-only PASS, a container PASS or a missing/shorter archive is not
evidence. See [`security-vm.md`](security-vm.md).
