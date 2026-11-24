# Marqdo CLI — current architecture

Status: **current architecture overview**. This file summarises the tree as it
is today. It is not a release promise, does not reopen a frozen ADR, and does not
replace `spec/` or a recorded test log.

- Baseline: `2c7c4f31e7a388d3fcfa717a77067e696ed13854` (2026-11-22, tagged `v3.0.0-beta.1`; same-SHA evidence in `../STATUS.md` and `../evidence/ci/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`).
- Normative behavior: `spec/`; frozen decisions: `docs/adr/`; executed evidence: `docs/STATUS.md` and `docs/evidence/`.
- This is the **single current overview**. Do not add a second running "current state" file;
  update this file, `docs/STATUS.md` and an evidence file together.

> **Language contract and implementation.** The language contract is
> [RFC-LANG-0002](RFC-LANG-0002-unified-semantics.md) (adopted by ADR-0052) and
> `spec/mapping.md`. The frontend, `marqdo-expression`, IR lowering, runtime and
> bundled `stdlib/` implement it. The workspace gate is green at `9c1723d`;
> rollout evidence and historical banners are indexed in
> [`../migration/README.md`](../migration/README.md).
>
> **Extensibility closure (RFC-EXT-0001).** The current tree implements the
> single static callable binding (ADR-0054), official stdlib public aliases
> replacing print magic (ADR-0058), third-party resource providers through the
> normal CLI (ADR-0055), the capability-handler registry (ADR-0057), and
> explicit extension selection with lock v4 (ADR-0056). The remaining PR-08
> action is a SHA-bound frozen-release rerun of the external conformance suite
> on the tagged commit; the suite PASSes against the release build of this tree.
> Normative boundaries: [`spec/callable-resolution.md`](../../spec/callable-resolution.md),
> [`spec/extension-selection.md`](../../spec/extension-selection.md),
> [`spec/resource-provider-v2.md`](../../spec/resource-provider-v2.md).
>
> **Post-baseline core/extension boundary (RFC-EXT-0003, ADR-0060–0063).** The
> core call surface no longer owns the domain math kernel (`math.*` removed;
> `crates/marqdo-math` deleted; ADR-0061), sibling heading mode inheritance is
> fixed (H-06), the release archive must ship the verified `stdlib/` (H-08), one
> `CORE_RESOURCE_KINDS` registry guards every provider registration path
> (H-04), `resource_request` operation ownership is enforced by an architecture
> gate (H-03), and the foreign one-shot primitive is explicitly retained as F-A
> (ADR-0063). Heading Annotations v1 is **Proposed/not implemented** (ADR-0060,
> target `3.1.x`) and must not be documented as available. Executed evidence:
> [`../STATUS.md`](../STATUS.md) §0.6 and
> [`../evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md`](../evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md).

> **Stable core review (RFC-GPL-0004).** [ADR-0064](../adr/0064-stable-core-extension-boundary.md)
> sets the review rule for builtins, HostOp, Value and top-level commands.
> Linux CI builds a release archive and exercises its installed executables
> against a fixed commit in the external conformance repository. The
> [Resource Control](../adr/0065-resource-control-vnext.md) and
> [Task v1](../adr/0066-structured-tasks-v1.md) designs are Proposed.
> See [STATUS §0.7](../STATUS.md#07-post-beta-stable-core-review--rfc-gpl-0004-unreleased).

> **Structured control flow (RFC-CTRL-0005).**
> [ADR-0067](../adr/0067-structured-control-flow.md) extends the payload parser
> and branch mapper, adds nearest-loop execution transfers and makes binding
> inference path-sensitive. Analysis projects the same branch and loop
> structure for every module. The interpreter reports transfers through Host
> observation.

## Public boundaries and pre-stable changes

[ADR-0068](../adr/0068-single-current-public-contracts.md) records the policy
for replacing public interfaces before the first stable release.

| Boundary | Owner | Current contract |
|---|---|---|
| Static facts | CLI Analysis projection of the prepared project | [Analysis](../../spec/analysis.md) |
| Runtime facts | Interpreter and Host observation sink | [Observation](../../spec/observation.md) |
| Installed CLI commands | Generic Tool registry | [CLI Tool](../../spec/cli-tool.md) |
| Runtime extensions | Plugin, Resource Provider and Foreign each enforce their own grants | [Plugin ABI](../../spec/plugin-abi.md), [Resource Provider](../../spec/resource-provider-v2.md), [Foreign](../../spec/foreign.md) |
| Persistent worker plumbing | Plugin and provider workers share one bounded framed transport; each keeps its vocabulary, limits and authority | [Shared worker transport](../../spec/worker-ipc.md) |
| Values and package identities | Value Wire and project lock | [Value Wire](../../spec/value-wire.md), [Extension selection and lock](../../spec/extension-selection.md) |

`check --emit analysis.json` projects static facts from the prepared project.
`run` and `call` execute the same prepared program through the CapabilityHost.
`--output jsonl` provides live events and `--observation-file run.json` saves
the same event sequence from that invocation. Consumers verify the exact
Analysis bytes before marking execution spans in source.

When replacing a contract, update its producer, specification, schema, real
consumers, fixtures and CI in one integration. Every required field and format
identifier should correspond to a check at the boundary. Reject obsolete
inputs with a corrective action. Keep historical formats in Git. Review
grants, budgets, file integrity, source attribution and privacy independently
of the transport change. Check the [implementation status](../STATUS.md)
for executed evidence and release gates.

## 1. Four state dimensions

A claim is stated in exactly one of these dimensions. "Exists in the tree" is not
the same as "tested here", "published in the bundled stdlib" or "verified on a
platform".

| Dimension | Current state | Source |
|---|---|---|
| Implemented | Rust workspace: RFC-LANG-0002 Markdown/frontend/IR, v2 statement parser, tree-walk runtime, `Memory`, capability host, Tool, Analysis and Observation, Plugin ABI, Foreign kernels, migrated bundled stdlib, CLI composition root; core/extension boundary RFC-EXT-0003 (ADR-0060–0063: math domain API removed, one core resource-kind registry, reviewed `resource_request`, F-A foreign retained), heading annotations only **Proposed** | `Cargo.toml`, `crates/`, `stdlib/` |
| Tested locally | `cargo test --workspace --locked`, `cargo fmt --all -- --check`, `clippy -D warnings`, release archive smoke and an independent `ubuntu:24.04` cold-install run on the RFC-EXT-0003 tree (Linux x86_64); exact counts and logs are in `docs/STATUS.md` §0.6 and the RFC-EXT-0003 evidence file. Historical baselines stay in earlier sections. | `docs/STATUS.md` §0.6, `docs/evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md` |
| Bundled stdlib released | In-repo bundle `marqdo-stdlib@0.1.0`, SHA-256 digest verified before loading; it is **not** an external registry package | `stdlib/stdlib.json`, `stdlib/README.md` |
| Multi-platform verified | Linux local gate is the only executed lane here; Windows/macOS native lanes are **UNVERIFIED**, and the GitHub security lane remains a **VM-only** gate | `docs/STATUS.md` §1, §4 |

## 2. Responsibility layers

```text
user .mq.md programs / agents / research workflows
                     |
stdlib/*.mq.md (versioned and digest-checked in this repository)
  core / collections / text / encoding / data
  io / console / stream / fs / net / process
  memory / alloc + portable pure algorithms
  (math algorithms are external: examples/math-library + ABI backend)
                     |
language semantics: Markdown -> AST -> frontend -> IR
                     |
execution: Value / MemoryStore / evaluator / budgets
  (computes; does not own stdout or file handles)
                     |
controlled platform API: open/read/write/close + non-streaming requests
  capability / budget / audit / observation
  stdio / file / tcp / OS adapters
  explicitly registered providers (in-process / worker / C ABI)
                     |
OS / external services / devices

CLI is only the composition root: parse arguments, load the project,
assemble Host + Runtime, route output, exit with a status.
`project_prepare::prepare` is the single finalisation step: it loads sources,
reads static extension descriptors (never `dlopen`/spawn), builds the unique
`CallableRegistry`, binds every call site once and hands the same
`PreparedProgram` to `check`/`run`/`call`/`inspect`/Analysis. The runtime
dispatches only by the resulting `CallBindingTable`; it never re-resolves a
name or guesses a plugin.
Callable Plugin / Foreign kernel / Tool Protocol are separate extension
categories; they do not enter the standard library or the language IR.
```

Layer rules:

1. The language core does not know `stdout`, a project directory, a TCP timeout
   or a third-party plugin path.
2. The platform layer owns I/O, clocks, entropy, sockets and file handles. It must
   not own JSON parsing, text formatting or memory allocation policy.
3. The stdlib is `.mq.md` and ships in this repository. New ordinary algorithms
   go to `stdlib/` + tests + docs; they do not become Rust builtins.
4. The CLI selects inputs, resolves configuration, assembles the host and routes
   human/machine output. It is not a semantic policy centre.
5. Callable Plugin and Resource Provider share one registry/authorization/audit
   mechanism, but keep different capability types.
6. CLI policy/commands are `commands/{sandbox,run,check,inspect,init,package,tool}.rs`;
   host open-path/providers/dispatch are
   `host/{resource_ops,providers,dispatch}.rs`; process execution is
   `process.rs`. These are child modules, not new crates or a second backend.

## 3. Dependency direction and public contracts

Current Rust dependency edges (documented, not proposed):

```text
marqdo-cli -> marqdo-runtime -> marqdo-host
marqdo-cli -> marqdo-package, marqdo-sandbox, marqdo-frontend, ...
marqdo-runtime -> marqdo-ir, marqdo-expression, marqdo-value
marqdo-host   -> marqdo-plugin-abi, marqdo-foreign, marqdo-sandbox, marqdo-worker-ipc
marqdo-plugin-abi     -> marqdo-worker-ipc
marqdo-provider-worker -> marqdo-worker-ipc
```

`marqdo-worker-ipc` depends only on `serde_json` and `thiserror`: it is the one
copy of bounded worker framing, deadlines and process teardown
([spec/worker-ipc.md](../../spec/worker-ipc.md)). It must never depend on the
host, the plugin ABI, or a worker binary; sharing plumbing must not share
capabilities. Foreign kernels and CLI Tools stay outside it.

`marqdo-runtime -> marqdo-host` is the current direction. Do not add a second
execution backend or a `marqdo-stdlib` crate by implication. A future embedded
runtime or a second backend needs the thin `ExecutionPort` contract described in the
review RFC, not a new crate per provider.

| Public contract | API (`.mq.md`) | State | Tests |
|---|---|---|---|
| stdio stdin | `resource.open("stdio","stdin",["read"])`, `resource.read`, `resource.close`; one run-scoped cursor | implemented, read-only | `resource_streams`, `p2_closure`, `tests/conformance` |
| Observation | `run/call --output jsonl`, `--output json` or `--observation-file PATH` | one event sequence and capture policy | `observation`, `analysis_observation`, `observation_consumer.py` |
| stdio stdout | `resource.open("stdio","stdout",["write"])`, `resource.write`, `resource.close`; separate output budget | implemented | `stdlib_bundle`, `print_routing`, `resource_streams` |
| stdio stderr | `resource.open("stdio","stderr",["write"])`; separate from stdout and CLI diagnostics | implemented | `resource_streams`, `stdlib_product` |
| project file read | `resource.open("file", path, ["read"])`; path/grant checked; bounded | implemented, project mode | `resource_streams`, `stdlib_product` |
| project file write | `resource.open("file", path, ["write"])`; buffers until `close`, never flush-on-drop | implemented, project mode | `resource_streams`, `stdlib_product` |
| `std:fs/file` | `读文本/写文本/复制文件/存在/元数据/列出目录/创建目录/删除/重命名/追加文本/读取片段/遍历` over `resource.*`; bounded at 8 MiB | implemented; no watch/binary offset read | `stdlib_product` |
| `resource.request` | the reviewed operation allowlist (`process.run`, `regex.*`, `compress.*`, `random.bytes`, `env.get`, `clock.now`, `fs.*`) in `docs/architecture/resource-operations.md`; project mode + grants, ADR-0062 | implemented; **new domain operations forbidden**; architecture gate pins the list | `resource_streams`, `architecture_boundaries::resource_request_operation_allowlist_is_reviewed` |
| core resource kinds | one `marqdo_host::CORE_RESOURCE_KINDS` registry used by CLI manifest validation and every host registration API | implemented; all six core kinds rejected as provider name/kind before registration (H-04) | `resource_streams`, `provider_manifest::every_core_kind_is_rejected_as_name_and_as_kind` |
| `math.*` domain API | removed after `3.0.0-beta.1`; call is `symbol.unknown`; `Formula` stays inert; external pure package + optional ABI backend; `math.compute` stays a plugin capability category (H-01/H-11) | **removed** from `BUILTINS`/`HostOp`/host/runtime/workspace; `examples/math-library/`, `examples/math-backend/` | `math_library`, `conformance_semantics`, `architecture_boundaries::domain_math_kernel_is_not_part_of_the_core` |
| Heading modes / annotations | `@exec`/`@knowledge` are the closed Core Modes; unmarked headings inherit the nearest ancestor (H-06); third-party heading annotations are **Proposed** metadata only, Analysis v3 unchanged (H-05/H-07) | core mode: implemented; annotations: Proposed/not implemented, target `3.1.x` | `heading_inheritance`, `units`, negative `sem.invalid_mode_marker`; future T-ANNO in `spec/heading-annotations-v1.md` |
| Foreign F-A | permission-gated one-shot `foreign.run`/`foreign.call` + dynamic `KernelRegistry`; F-B removal deferred (H-02) | retained; no new language-specific builtin/HostOp; existing grants/digest/timeout/wire/output negatives unchanged | `marqdo-foreign/tests/kernels`, `smoke.py` foreign/kernel cases, `examples/kernel-mock` |
| `std:data/json` | `json.解析/解析多个/格式化`; nested Value, 128-depth bound, recoverable errors | implemented (recursive, multi-value; no streaming) | `stdlib_product` |
| `std:data/csv` | `csv.解析行/解析/格式化行/格式化`; quoted single/multi-row, CRLF, quoted newlines | implemented (no streaming) | `stdlib_product` |
| `std:path` | `path.连接/文件名/目录名/扩展名/规范/是否绝对`; lexical only | implemented (no I/O) | `stdlib_product` |
| `std:regex` | pure `匹配/搜索` subset; host `扩展匹配/捕获` with groups/alternation/captures | implemented both paths; pure path has no groups | `resource_streams`, `stdlib_product` |
| `std:datetime` | `datetime.格式化(毫秒)` / `datetime.解析(文本)`; UTC ISO-8601, pre-1970 supported | implemented (pure) | `stdlib_product` |
| `std:clock` | `clock.现在()` -> `resource.request("clock.now", [])` | implemented (project mode; pure/strict denied) | `resource_streams`, `stdlib_product` |
| `std:env` | `env.读取/存在` -> `resource.request("env.get", [name])`; exact `--allow-env` grant | implemented (project mode; no `env.list`) | `resource_streams`, `stdlib_product` |
| `std:process` | `process.运行(程序, 参数列表)` -> `resource.request("process.run", ...)`; exact `--allow-process` grant, 5 s timeout, 1 MiB caps | implemented (project mode; not an OS sandbox) | `resource_streams`, `stdlib_product` |
| `std:random` | `random.字节/整数` -> `resource.request("random.bytes", [count])`; exact `--allow-random` grant | implemented (project mode; byte-list RNG) | `resource_streams`, `stdlib_product` |
| `std:compress` | `compress.压缩/解压` -> gzip byte list <-> Text; 8 MiB caps | implemented (project mode; no grant) | `resource_streams`, `stdlib_product` |
| network endpoints | `tcp`/`udp`/`tcp-listener`/`tls` open + `resource.accept`; exact host grant, DNS after grant, TLS certificate validation | implemented (project mode) | `resource_streams` |
| `std:net/http` | byte-exact HTTP/1.1 parse/build; chunk sizes are byte counts; text wrapper compatible | implemented (pure `.mq.md`) | `stdlib_http` |
| HTTP/1.1 | `stdlib/net/http.mq.md`: status/header/chunked-byte parse, request build | implemented over an already-authorized TCP stream; not a DNS/TLS client | `stdlib_http`, `http_over_tcp`, `stdlib_product` |
| print / official stdlib aliases | `打印`/`print` are verified `stdlib.json` public aliases bound to `io.mq.md::写值`; ordinary Marqdo calls, no `print_override`, no runtime branch, no `HostOp::Print` | implemented (ADR-0058) | `stdlib_alias_routing`, `print_routing`, `stdlib_product` |
| callable resolution | one registry + one `bind_program_calls` pass; `check`/`run`/`inspect`/Analysis share `CallSiteKey` bindings; plugin exports reconcile exactly with static manifests; no runtime name fallback | implemented (ADR-0054) | `callable_resolution`, `plugin_worker`, `analysis_observation` |
| resource provider v2 CLI | `--provider`/`--allow-provider` select and authorize a `provider.json` schema-2 `c-abi-1` worker artifact; manifest/artifact/grants/duplicate kinds verified before spawn; sibling helper only | implemented linux/x86_64; other platforms fail closed (ADR-0055) | `provider_cli` |
| capability handlers | trusted handler registry executes granted capabilities under budget/audit; missing handler is `capability.unavailable`; `fs.read` is the first real handler | implemented (ADR-0057) | `capability_handlers` |
| strict mode | only the pure/Math execution path and the built-in stdout console channel; project/foreign/plugin/tool/trace/file/network grants are rejected | implemented L1; L2 not implemented | `tests/security`, `resource_streams`, `stdlib_product` |

## 4. Extension path classification

| Extension path | Responsibility | State |
|---|---|---|
| Tool Protocol | external `marqdo` CLI tools, inspect/verify/IDE commands; versioned manifest and install/remove | implemented v1; not a standard-I/O replacement inside a user program |
| Callable Plugin ABI | native callable functions and controlled experimental acceleration | implemented v1 (native + worker); static manifest is the contract and run reconciles exports exactly; not an ordinary JSON/CSV formatter |
| Resource Provider | new resource kinds/devices/network/file endpoints | v2 CLI entry implemented for trusted worker transport on linux/x86_64 (ADR-0055); v1 in-process/trusted C ABI remain embedder APIs; not one provider per high-level algorithm |
| Foreign kernel | explicit cross-language compatibility for Python/Node one-shot execution | implemented; registry accepts an explicitly declared unknown language; not a Marqdo implementation of common algorithms |
| Official stdlib public alias | language-surface aliases such as `打印`/`print` declared in verified bundle metadata | implemented (ADR-0058); third parties cannot claim these names |
| Extension selection / lock v4 | `marqdo.yaml.extensions` activation sources; lock v4 manifest+artifact identity; `inspect --extensions` | implemented (ADR-0056); declaration is never a grant, `pure`/`strict` refuse third-party activation |
| Shared registry/policy/budget/audit/trace | package/adapter identity, descriptor/version checks, per-request grants, timeouts, error classes and trace metadata | housed inside `marqdo-host`; do not invent a shared binary ABI for function calls and resource streams |

## 5. Gap and closure backlog

This is the honest remaining work, ordered by the review RFC. It is not a claim
that a phase is complete.

| Priority | Gap | State |
|---|---|---|
| P0 | **Language syntax migration**: RFC-LANG-0002 single contract | **closed at `9c1723d`**; implementation, stdlib digest, examples and fixtures green; platform/strict evidence remains separate |
| P0 | Stdlib release is a bundle in this repo; extraction is deferred (ADR-0039) | accepted, keep in repo |
| P0 | Documentation drift between `STATUS.md`, the architecture index and the evidence | being fixed by this migration and the docs gate |
| P1 | `std:fs/file` read/write/copy, metadata/directory, append/read_at/walk are implemented; watch and binary offset reads remain open | open |
| P1 | No file watch or binary `read_at` | open; future Resource Operation extension |
| P1 | JSON/CSV product parsers are implemented with depth bound and multi-value/multi-row; no incremental streaming parser | open |
| P1 | CLI split into `commands/{sandbox,run,check,inspect,init,package,tool}.rs`; host split into `host/{resource_ops,providers,dispatch}.rs` plus `process.rs`; `commands.rs` keeps shared project/runtime helpers and `host.rs` keeps the `Host` trait methods and capability checks | split; shared helpers and trait methods remain by design |
| P2 | HTTP/1.1 framing is byte-exact; HTTP/2, trailers and transfer decoding remain open | open |
| P2 | DNS, UDP, TCP listener and TLS client endpoints are implemented; no TLS server/mTLS or custom-CA CLI | open |
| P2 | `std:path`, pure+host regex, `std:datetime`, `std:clock`, `std:env`, `std:process`, `std:random` and gzip compression are implemented; no streaming/zstd/brotli | open |
| P2 | Memory limits: 16 MiB per region, 64 MiB per run | current budget, not a product limit |
| P3 | No concurrency tasks, atomics or cancellable long-running scheduling | open; separate ADR |
| P3 | No native optimizer/JIT or columnar numeric backend | open; benchmark-first |
| P3 | Windows Native/Worker ABI is unsupported; macOS/Windows CI is unverified | open |
| P0 | RFC-EXT-0001 PR-04: `marqdo.yaml.extensions` + lock v4 extension selection, CLI composition-root wiring, `inspect --extensions` | **implemented/tested**; see `spec/extension-selection.md`, ADR-0056, `extension_selection` tests |
| P0 | RFC-EXT-0001 PR-08: frozen release binary rerun on the tagged commit + SHA-bound evidence manifest | **remaining release step** (`tools/ci/verify_extension_contracts.py`) |
| P1 | Provider worker transport only linux/x86_64; macOS/Windows provider lanes fail closed | open; separate platform work |

## 6. Documentation governance

1. There is one current overview (this file), one executed-state file
   (`docs/STATUS.md`), frozen decisions in `docs/adr/`, and fact snapshots in
   `docs/evidence/`. Do not copy a running status into several planning docs.
2. A design input (a proposed RFC or a historical planning doc) must say
   **Proposed** or **Historical** in its own header. “Proposed” never means
   “implemented”.
3. Every status row in `docs/STATUS.md` must name an executed command, a log or a
   source path. Do not hand-edit a gate result without an evidence update.
4. Before committing a documentation change, run:
   `cargo test -p marqdo-cli --test docs_consistency` and
   `git diff --check`. A broken relative link or a stale “current” claim fails
   the gate.
5. When behavior or status changes, update code + tests + `docs/STATUS.md` +
   this file + an evidence file in the same change. A fixture/demo is not a
   published stdlib API.
6. A public-boundary change must follow
   [`PROTOCOL_CONVERGENCE_POLICY.md`](PROTOCOL_CONVERGENCE_POLICY.md): one
   current contract, replace in place, reject retired inputs, no compatibility
   path or version negotiation, and no version bump without a shape change.

## 7. References

- [Architecture index](README.md) — RFC/ADR/evidence history.
- [Protocol convergence policy](PROTOCOL_CONVERGENCE_POLICY.md) — one-contract rules, anti-patterns and review checklist.
- [Implementation status](../STATUS.md) — executed state and blockers.
- [Language/frontmatter blueprint](../DEVELOPMENT_BLUEPRINT.md) — design baseline; not current status.
- [3.0 prerelease plan](../MARQDO_3_0_PRERELEASE_PLAN.md) — current release policy and beta gates.
- [Stable-release implementation plan](../STABLE_RELEASE_IMPLEMENTATION.md) — historical stable-release roadmap input; not current status.
- [ADR-0041: non-streaming resource operations](../adr/0041-resource-operation-fs.md) — accepted `resource.request` `fs.*` contract.
- [ADR-0042: `clock.now` resource operation](../adr/0042-clock-resource-operation.md) — accepted project-mode Unix-millis clock.
- [ADR-0043: `env.get` resource operation](../adr/0043-env-resource-operation.md) — accepted exact-name environment grants.
- [ADR-0044: `process.run` resource operation](../adr/0044-process-resource-operation.md) — accepted exact-path process grant; not a sandbox.
- [ADR-0045: fs append/read_at/walk](../adr/0045-fs-append-read-at-walk.md) — accepted bounded content and directory walk.
- [ADR-0046: `random.bytes`](../adr/0046-random-resource-operation.md) — accepted explicitly granted OS entropy.
- [ADR-0047: DNS/UDP/TCP listener](../adr/0047-net-dns-udp-listener.md) — accepted project-mode network endpoints.
- [ADR-0048: TLS client endpoint](../adr/0048-tls-resource-endpoint.md) — accepted validated rustls client; system + embedder DER roots.
- [ADR-0049: gzip compression](../adr/0049-compression-resource-operations.md) — accepted bounded gzip byte-list codec.
- [ADR-0050: host regex extension](../adr/0050-host-regex-extension.md) — accepted project-mode groups/alternation/captures.
- [ADR-0052: byte-exact HTTP framing](../adr/0051-byte-exact-http-framing.md) — accepted byte-list HTTP/1.1 parser/builder.
- [Compute-machine RFC-0001](../MARQDO_COMPUTE_MACHINE_RFC_0001.md) and [core/stdlib RFC-0002](../MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md) — design input.
- [Architecture audit and refactor RFC](Marqdo_CLI_Latest_Architecture_Audit_and_Refactor_RFC.md) — review input for this migration.
- [RFC-EXT-0001](RFC-EXT-0001.md) — extensibility closure task specification (implementation status tracked here and in STATUS).
- [ADR-0054: single callable binding](../adr/0054-callable-binding-single-source.md) — one registry/binding pass, runtime dispatch by identity.
- [ADR-0055: resource provider CLI v2](../adr/0055-resource-provider-cli-v2.md) — provider v2 manifest/worker CLI closure and trust boundary.
- [ADR-0057: capability handler registration](../adr/0057-capability-handler-registration.md) — declared capability vs installed handler vs audit.
- [ADR-0058: stdlib public aliases](../adr/0058-stdlib-public-alias.md) — verified metadata replaces print magic.
- [`spec/callable-resolution.md`](../../spec/callable-resolution.md) and [`spec/resource-provider-v2.md`](../../spec/resource-provider-v2.md) — normative extension boundaries.
