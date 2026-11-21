# Marqdo CLI — current architecture

Status: **current architecture overview**. This file summarises the tree as it
is today. It is not a release promise, does not reopen a frozen ADR, and does not
replace `spec/` or a recorded test log.

- Baseline: [`9c1723d44f403d6b742136f7881515083b413cc2`](https://github.com/cflmy/marqdo-cli/commit/9c1723d44f403d6b742136f7881515083b413cc2) (2026-11-21, RFC-LANG-0002 implementation and follow-up fixes; language code baseline).
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

## 1. Four state dimensions

A claim is stated in exactly one of these dimensions. "Exists in the tree" is not
the same as "tested here", "published in the bundled stdlib" or "verified on a
platform".

| Dimension | Current state | Source |
|---|---|---|
| Implemented | Rust workspace `0.1.0`: RFC-LANG-0002 Markdown/frontend/IR, v2 statement parser, tree-walk runtime, `Memory`, capability host, Tool/Analysis/Trace, Plugin ABI, Foreign kernels, migrated bundled stdlib, CLI composition root | `Cargo.toml`, `crates/`, `stdlib/` |
| Tested locally | `cargo test --workspace --locked`: **599 passed, 0 failed** across 67 targets on Linux x86_64 in the RFC-EXT-0001 tree; fmt and clippy clean. Historical baseline: 507 passed at `9c1723d`. | `docs/STATUS.md`, `docs/evidence/2026-11-22-ext-implementation.md` |
| Bundled stdlib released | In-repo bundle `marqdo-stdlib@0.1.0`, SHA-256 digest verified before loading; it is **not** an external registry package | `stdlib/stdlib.json`, `stdlib/README.md` |
| Multi-platform verified | Linux local gate is the only executed lane here; Windows/macOS native lanes are **UNVERIFIED**, and the GitHub security lane remains a **VM-only** gate | `docs/STATUS.md` §1, §4 |

## 2. Responsibility layers

```text
user .mq.md programs / agents / research workflows
                     |
stdlib/*.mq.md (versioned and digest-checked in this repository)
  core / collections / text / encoding / data / math
  io / console / stream / fs / net / process
  memory / alloc + portable pure algorithms
                     |
language semantics: Markdown -> AST -> frontend -> IR
                     |
execution: Value / MemoryStore / evaluator / budgets
  (computes; does not own stdout or file handles)
                     |
controlled platform API: open/read/write/close + non-streaming requests
  capability / budget / audit / trace
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
   host open-path/providers/effects/dispatch are
   `host/{resource_ops,providers,effects,dispatch}.rs`; process execution is
   `process.rs`. These are child modules, not new crates or a second backend.

## 3. Dependency direction and public contracts

Current Rust dependency edges (documented, not proposed):

```text
marqdo-cli -> marqdo-runtime -> marqdo-host
marqdo-cli -> marqdo-package, marqdo-sandbox, marqdo-frontend, ...
marqdo-runtime -> marqdo-ir, marqdo-expression, marqdo-value
marqdo-host   -> marqdo-plugin-abi, marqdo-foreign, marqdo-sandbox, marqdo-math
```

`marqdo-runtime -> marqdo-host` is the current direction. Do not add a second
execution backend or a `marqdo-stdlib` crate by implication. A future embedded
runtime or a second backend needs the thin `ExecutionPort` contract described in the
review RFC, not a new crate per provider.

| Public contract | API (`.mq.md`) | State | Tests |
|---|---|---|---|
| stdio stdin | `resource.open("stdio","stdin",["read"])`, `resource.read`, `resource.close`; one run-scoped cursor | implemented, read-only | `resource_streams`, `p2_closure`, `tests/conformance` |
| stdio stdout | `resource.open("stdio","stdout",["write"])`, `resource.write`, `resource.close`; separate output budget | implemented | `stdlib_bundle`, `print_routing`, `resource_streams` |
| stdio stderr | `resource.open("stdio","stderr",["write"])`; separate from stdout and CLI diagnostics | implemented | `resource_streams`, `stdlib_product` |
| project file read | `resource.open("file", path, ["read"])`; path/grant checked; bounded | implemented, project mode | `resource_streams`, `stdlib_product` |
| project file write | `resource.open("file", path, ["write"])`; buffers until `close`, never flush-on-drop | implemented, project mode | `resource_streams`, `stdlib_product` |
| `std:fs/file` | `读文本/写文本/复制文件/存在/元数据/列出目录/创建目录/删除/重命名/追加文本/读取片段/遍历` over `resource.*`; bounded at 8 MiB | implemented; no watch/binary offset read | `stdlib_product` |
| `resource.request` | `resource.request("fs.exists|metadata|list|mkdir|remove|rename", paths)`; project mode, grant-checked, recoverable fs Result | implemented `fs.*`; no seek/append/watch | `resource_streams`, `stdlib_product` |
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
| callable resolution | one registry + one `bind_program_calls` pass; `check`/`run`/`inspect`/Analysis share `CallSiteKey` bindings; plugin exports reconcile exactly with static manifests; no runtime name fallback | implemented (ADR-0054) | `callable_resolution`, `plugin_worker`, `analysis_trace` |
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
| P1 | CLI split into `commands/{sandbox,run,check,inspect,init,package,tool}.rs`; host split into `host/{resource_ops,providers,effects,dispatch}.rs` plus `process.rs`; `commands.rs` keeps shared project/runtime helpers and `host.rs` keeps the `Host` trait methods and capability checks | split; shared helpers and trait methods remain by design |
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

## 7. References

- [Architecture index](README.md) — RFC/ADR/evidence history.
- [Implementation status](../STATUS.md) — executed state and blockers.
- [Language/frontmatter blueprint](../DEVELOPMENT_BLUEPRINT.md) — design baseline; not current status.
- [Stable-release implementation plan](../STABLE_RELEASE_IMPLEMENTATION.md) — next-phase plan; not implemented behavior.
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
