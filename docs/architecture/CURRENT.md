# Marqdo CLI — current architecture

Status: **current architecture overview**. This file summarises the tree as it
is today. It is not a release promise, does not reopen a frozen ADR, and does not
replace `spec/` or a recorded test log.

- Baseline: [`24c88bcc78b1c705eb2319e7aaa8822a415eb01f`](https://github.com/cflmy/marqdo-cli/commit/24c88bcc78b1c705eb2319e7aaa8822a415eb01f) (2026-11-21, before the documentation migration commit).
- Normative behavior: `spec/`; frozen decisions: `docs/adr/`; executed evidence: `docs/STATUS.md` and `docs/evidence/`.
- This is the **single current overview**. Do not add a second running "current state" file;
  update this file, `docs/STATUS.md` and an evidence file together.

> **Language contract vs implementation.** The current language contract is
> [RFC-LANG-0002](RFC-LANG-0002-unified-semantics.md) (adopted by ADR-0052) and
> `spec/mapping.md`. The Rust frontend, expression parser and bundled `stdlib/`
> still implement the pre-RFC mapping; source/test/doc migration is tracked in
> [`../migration/README.md`](../migration/README.md). Do not read this overview
> as proof that the new syntax is executable.

## 1. Four state dimensions

A claim is stated in exactly one of these dimensions. "Exists in the tree" is not
the same as "tested here", "published in the bundled stdlib" or "verified on a
platform".

| Dimension | Current state | Source |
|---|---|---|
| Implemented | Rust workspace `0.1.0`: Markdown/frontend/IR, tree-walk runtime, `Memory`, capability host, Tool/Analysis/Trace, Plugin ABI, Foreign kernels, bundled stdlib, CLI composition root | `Cargo.toml`, `crates/`, `stdlib/` |
| Tested locally | `cargo test --workspace --locked`: **491 passed, 0 failed, 2 ignored** on Linux x86_64 at the baseline | `docs/STATUS.md` §3 |
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
| file metadata / directory / rename / remove / mkdir | not part of the current Resource contract | **not implemented**; needs a separate Resource Operation ADR | — |
| TCP | `resource.open("tcp", ip:port, ["read","write"])`; exact IP grant, 5 s timeout | implemented; no DNS/TLS/server socket/UDP | `tcp_endpoint`, `http_over_tcp` |
| HTTP/1.1 | `stdlib/net/http.mq.md`: status/header/chunked-byte parse, request build | implemented over an already-authorized TCP stream; not a DNS/TLS client | `stdlib_http`, `http_over_tcp`, `stdlib_product` |
| print | bundled `io.mq.md`/`console.mq.md` through `resource.write`; legacy `HostOp::Print` removed | implemented (Compat-C) | `print_routing`, `stdlib_product` |
| strict mode | only the pure/Math execution path and the built-in stdout console channel; project/foreign/plugin/tool/trace/file/network grants are rejected | implemented L1; L2 not implemented | `tests/security`, `resource_streams`, `stdlib_product` |

## 4. Extension path classification

| Extension path | Responsibility | State |
|---|---|---|
| Tool Protocol | external `marqdo` CLI tools, inspect/verify/IDE commands; versioned manifest and install/remove | implemented v1; not a standard-I/O replacement inside a user program |
| Callable Plugin ABI | native callable functions and controlled experimental acceleration | implemented v1 (native + worker); not an ordinary JSON/CSV formatter |
| Resource Provider | new resource kinds/devices/network/file endpoints | implemented v1 in-process, worker and trusted C ABI; not one provider per high-level algorithm |
| Foreign kernel | explicit cross-language compatibility for Python/Node one-shot execution | implemented; not a Marqdo implementation of common algorithms |
| Shared registry/policy/budget/audit/trace | package/adapter identity, descriptor/version checks, per-request grants, timeouts, error classes and trace metadata | housed inside `marqdo-host`; do not invent a shared binary ABI for function calls and resource streams |

## 5. Gap and closure backlog

This is the honest remaining work, ordered by the review RFC. It is not a claim
that a phase is complete.

| Priority | Gap | State |
|---|---|---|
| P0 | **Language syntax migration**: RFC-LANG-0002 is the single contract, but the frontend/expression/stdlib implementation and executable fixtures are still on the pre-RFC mapping | **in progress**; docs/examples/test text migrated, implementation pending; see `docs/migration/` |
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
