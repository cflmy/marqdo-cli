# RFC-EXT-0001 baseline audit — `595279f`

Status: **baseline evidence recorded before implementation.** This is not a
release claim and none of the new RFC-EXT-0001 behaviour existed at this SHA.

## 1. Identity and environment

| Field | Value |
|---|---|
| Repository | `cflmy/marqdo-cli` (private remote `https://git.cflmy.cn/cflmy/marqdo-cli`) |
| Baseline commit | `595279fe38048b52b96d9ff1e7d57a867ec5c4c6` (`master`, 2026-11-21) |
| Working branch | `feat/extensible-ecosystem` |
| Rust | `rustc 1.100.0 (0ef1bc386 2026-11-09)` |
| Cargo | `cargo 1.100.0 (3acc79c0c 2026-11-05)` |
| OS / arch | Linux `x86_64` (Ubuntu container) |
| Local homes | `RUSTUP_HOME=.rustup-home`, `CARGO_HOME=.cargo-home` |
| Command log | `.cache/ext/baseline.log` (local, gitignored) |

## 2. Executed gate (same SHA, before any RFC-EXT-0001 edit)

```text
cargo fmt --all -- --check                            exit 0  PASS
cargo clippy --workspace --all-targets -- -D warnings exit 0  PASS
cargo test --workspace --locked                       exit 0  PASS
  62 test targets: 521 passed, 0 failed, 2 ignored
```

The RFC §2.2 static inventory commands were also run and their findings are
recorded in §3. A green baseline is only the starting point, not evidence for
any new feature.

## 3. Source facts at the baseline (RFC §2.1 refresh)

Each row was checked against the actual tree, not against the RFC prose. None of
the rows was `already_closed` at `595279f`.

| ID | Observed at `595279f` | Impact | RFC treatment |
|---|---|---|---|
| EXT-01 | `marqdo-frontend/src/units.rs` `SemanticMode` is `Exec`/`Knowledge` only; unknown `@xxx` is `sem.invalid_mode_marker` | third parties cannot add an execution mode (intended closure) | keep; metadata is inert Markdown |
| EXT-02 | `frontend/src/calls.rs` checks Member builtin before import; `runtime/src/interpreter.rs` checks import before builtin | check and run can bind different targets | PR-01/02 single binding |
| EXT-03 | `ir/src/callable.rs` has `CallableRegistry` but calls still use `builtins::find` and runtime fallbacks | registry was not the single truth | PR-01/02 |
| EXT-04 | interpreter tolerated undeclared `alias.name` as a plugin call when `external_callables` was empty | static-unknown symbol failed only at run time | PR-02 remove fallback |
| EXT-05 | `cli/src/commands/run.rs` synthesised `params: Vec::new()` descriptors for loaded exports | manifest/runtime signature could differ silently | PR-02 exact reconciliation |
| EXT-06 | `loader.rs` validated the program before the CLI appended static plugin descriptors, then validated again | duplicated diagnostics and ordering hazards | PR-01 single `prepare` finalize |
| EXT-07 | `host/src/host/providers.rs` has Rust/C/Worker provider registration; CLI had no `--provider` | providers were only reachable by trusted embedders | PR-03 |
| EXT-08 | `host/src/capability.rs::execute` implemented only `fs.read`; other names returned unavailable | declaring a capability did not mean executing it | PR-05 handler registry |
| EXT-09 | `host/src/host.rs::resource_request` has per-string built-in operations | adding a non-streaming operation required core edits | PR-03/05; keep core operations |
| EXT-10 | `cli/src/loader.rs::program_uses_print` injected `stdlib/io.mq.md` and set `Program.print_override`; runtime special-cased `打印/print` | migrating another stdlib function by copying this pattern would hard-code every name | PR-06 public aliases |
| EXT-11 | `foreign/src/lib.rs::KernelRegistry` supports unknown languages; old `normalize_language` only knows Python/Node | registry is the execution path, legacy helper is display-only | keep registry as the only query |
| EXT-12 | Plugin ABI v1 Native/Worker implemented; Wasm unsupported; Tool Protocol tested | do not rebuild plugin/tool platform | keep + add identity reconciliation |
| EXT-13 | `package/src/lib.rs` supports path + pinned `git` + package instances; `stdlib/` is a verified bundle | real `.mq.md` source decoupling exists | keep + external black-box test |
| EXT-14 | strict L1 permits only limited pure computation and stdout and rejects Worker/Foreign/Provider | project mode is not an OS sandbox | keep fail-closed; strict L2 is a separate RFC |

## 4. Issues reproduced while recording this baseline

They were reproduced while writing the RFC acceptance fixtures; all are fixed by
the PR series recorded later in `docs/STATUS.md` and
`docs/evidence/extension/<SHA>/manifest.json`:

1. **Duplicate resolution (EXT-02/03/04).** A fixture with an import alias that
   matched a builtin dotted namespace (`import: {math: ./math.mq.md}` plus
   `math.eval(...)`) had no deterministic binding rule; the frontend checked the
   builtin while the runtime checked the import first.
2. **Runtime plugin guessing (EXT-04).** `solver.add(...)` without `--plugin`
   reached the runtime and attempted `PluginInvoke` instead of failing during
   `check` with `symbol.unknown`.
3. **Synthesised plugin signatures (EXT-05).** `run` could execute a plugin
   export whose static descriptor had an empty parameter list, so an arity or
   position/name mismatch was only discovered (or not) at the ABI boundary.
4. **Print magic (EXT-10).** `打印`/`print` were routed by a whole-AST
   `program_uses_print` scan plus a runtime name special case; there was no
   generic de-magicking path for a second official stdlib function.
5. **Provider CLI gap (EXT-07).** There was no `--provider`/`--allow-provider`;
   an external `provider.json` could not be used through the shipped binary.

## 5. Not claimed by this baseline

* No `--provider`/`--allow-provider` existed; strict L2 remains unimplemented.
* No frozen-binary external repository evidence existed.
* No `docs/evidence/extension/<SHA>/manifest.json` existed.
* G3 remote security and G4 performance status is unchanged by this document;
  see `docs/STATUS.md` and `docs/releasing.md`.
