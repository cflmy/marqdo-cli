# ADR-0006: CLI output, exit-code and JSON contract

> Historical public protocol sections. Current Analysis, runtime output and
> Tool contracts are specified by [ADR-0068](0068-single-current-public-contracts.md).

Status: **Accepted**
Date: 2026-11-20

## Context

CI, scripts and coding agents need a stable CLI contract, not just human
output. The blueprint (§12) requires: all commands available with `--json`,
non-zero error exit codes, program stdout kept separate from diagnostics, and
machine-readable error codes. The implemented CLI must be documented precisely
because scripts will depend on stream separation and exit status.

Implementation: `crates/cli/src/main.rs` (clap definitions, process exit),
`crates/cli/src/commands.rs` (behavior), `crates/cli/src/output.rs`
(diagnostic/JSON rendering), `crates/cli/src/args.rs` (`--arg` constants).

## Decision

### 1. Commands and flags (implemented surface)

| Command | Arguments / flags | What it does |
|---|---|---|
| `init [dir]` | — | create `marqdo.yaml`, `index.mq.md`, `.gitignore`; refuses an existing manifest |
| `run [file]` | `--mode`, `--allow-read`, `--allow-write`, `--allow-foreign`, `--allow-plugin`, `--fuel`, `--stack`, `--max-output`, `--timeout`, `--plugin`, `--trust-native-plugin`, `--json`, `--trace` | execute `main` (or default `index.mq.md`) |
| `call <file> <function>` | `--arg NAME=VALUE` (repeatable) plus all `run` flags | execute one named function |
| `check [file]` | `--json` | static check; never executes |
| `inspect [file]` | `--ast`, `--units`, `--signature`, `--ir`, `--refs`, `--platform`, `--json` | static inspection; never executes |
| `fmt [file]` | `--check` | normalize CRLF and trailing newlines |
| `plugin inspect <dir-or-yaml>` | `--json` | validate manifest + verify artifact hash; never loads code |
| `add <name>` | `--path`, `--git`, `--rev`, `--json` | edit `marqdo.yaml` |
| `remove <name>` | `--json` | edit `marqdo.yaml` (and prune a stale lock entry) |
| `lock` | `--json` | write deterministic `marqdo.lock` |
| `sync` | `--locked`, `--json` | verify the lock is current |

There is no `test`, `tree`, `update`, `vendor`, `add` for plugins, `kernel`
command or `--project`/`--offline` flag in this build. `run` defaults the
file to `index.mq.md`; `check`/`inspect`/`fmt` do too. `call` requires an
explicit file path plus a function name.

`--mode` is only on `run`/`call` and accepts `pure|project|strict`
(default `project`). `--trust-native-plugin`/`--plugin` are only meaningful on
`run`/`call`. `inspect --platform` still requires a document argument (or
defaults to `index.mq.md`), because project loading happens first.

### 2. Exit codes

| Code | Meaning |
|---|---|
| `0` | success (for `check`: no **error** diagnostics; warnings do not fail) |
| `1` | a checked document has errors; a runtime/security failure; strict refusal; plugin load/inspect failure; lock/sync mismatch; `fmt --check` would reformat; `init` found an existing manifest; `add` duplicate; `remove` missing dependency; `sync` stale lock |
| `2` | usage error (clap), unreadable entry file, invalid `--mode` name, invalid capability combination (e.g. `--allow-read` in `pure`), invalid `--arg` value, unreadable manifest for project commands, unreadable file for `fmt` |

Codes are stable contract. Diagnostic **codes** (for example
`markdown.invalid_construct`, `expr.parse`, `symbol.unknown`,
`permission.denied`, `ref.ambiguous`) are stable; message wording is not.

> **Historical diagnostic note.** `markdown.invalid_construct` (and the
> executable-shape `Strong`/`Emph`/list/quote errors it described) belongs to
> the pre-RFC-LANG-0002 mapping. Under the current contract
> ([RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md) /
> [ADR-0052](0052-unified-markdown-semantics.md),
> [`spec/mapping.md`](../../spec/mapping.md) §8) syntax-role failures use the
> `sem.*` codes. The rest of this CLI contract is unaffected.

### 3. Stream separation

* **Program output** (`打印`/`print`) goes to stdout in text mode. In JSON mode
  it is embedded as the `stdout` string; it is never interleaved with JSON.
* **Diagnostics and effect traces** go to stderr in text mode.
* `run` in text mode writes only program output to stdout (no implicit return
  value). `call` in text mode additionally prints the returned value (using
  `Value::display`) when it is not `Unit`.
* `check` text mode: diagnostics to stderr; `checked N module(s): ok` to stdout
  on success; a `check failed: ...` summary to stderr on failure.
* `inspect` prints pretty JSON-ish structures to stdout; load errors to stderr.
* JSON mode prints exactly one pretty JSON object to stdout per invocation.
  `add`/`remove`/`lock`/`sync` still print errors as plain `error: ...` on
  stderr even with `--json` (JSON success envelope only) — an inconsistency to
  fix before the contract is declared final.

### 4. JSON output shapes

`check`:

```json
{"command":"check","success":true,"errors":0,
 "diagnostics":[{"file":"index.mq.md","module":"index.mq.md",
   "line":3,"column":5,"severity":"error","code":"expr.parse",
   "message":"...","hints":["..."]}]}
```

`inspect`:

```json
{"command":"inspect","module":"index.mq.md",
 "ast":{...},"units":[...],"signatures":[...],"ir":[...],
 "refs":[...],"platform":{...}}
```

Only the requested sections are present. With no section flags, a `summary`
with `modules`, `functions`, `units`, `resources` is emitted instead.

`run`/`call` (success):

```json
{"command":"run","success":true,
 "value":{"type":"text","value":"..."},
 "stdout":"program stdout text",
 "trace":[{"op":"print","module":"index.mq.md",
           "span":{"start":43,"end":57},"duration_micros":3,"result":"ok"}]}
```

Note: `call --json` also emits `"command":"run"` in this build (shared
implementation). Consumers should accept both names or key on the presence of
`diagnostics`/`value`; this is a known quirk.

`run`/`call` (failure): `{"command":"run","success":false,"stdout":...,
"error":{"code":"...","origin":"source|callsite","module_id":"...",
"span":{"start":..,"end":..},"line":..,"column":..,"file":..,
"frames":[...]},"trace":[...]}`. Runtime locations are resolved against the
logical module that produced the error (ADR-0017); there is no entry-file
fallback. Unknown modules or invalid spans leave line/column/file null while
keeping the module id and byte span.

`plugin inspect`:

```json
{"command":"plugin.inspect","success":true,"plugin":"demo.solver",
 "version":"0.1.0","abi":"^1.0","abi_compatibility":"compatible",
 "target":"x86_64-unknown-linux-gnu",
 "artifact":{"file":"solver.so","path":"...","sha256":"...","verified":true},
 "exports":[{"name":"solver.add","signature":"($a: Int, $b: Int) -> Int"}],
 "loaded":false}
```

`load_project` failures with `--json` print `{"success":false,"error":"..."}`
without a `command` field.

There is no `json_schema_version` field yet. The blueprint asks for a
versioned JSON schema; until it exists, treat these keys as the stable subset
and ignore unknown keys. Adding keys is not considered breaking; renaming or
removing one is.

### 5. `--arg` values

`--arg NAME=VALUE` parses `VALUE` with the **expression grammar restricted to
constants** (`crates/cli/src/args.rs`): integers, floats, double-quoted text
(escapes supported), `true`/`false`/`None`, lists, records, and unary `-`/`+`
over constants. Calls and identifiers are rejected ("must be a literal
constant"). Text must be double-quoted; there is no bare-word fallback.

### 6. Determinism and safety of read-only commands

* `check`/`inspect` never execute project code (ADR context; see
  `spec/security.md` §7).
* `plugin inspect` never loads plugin code.
* `fmt` only rewrites the file when not `--check`; exit 1 when `--check`
  detects a difference.

## Alternatives

1. **Use exit code 1 for all failures.** Rejected: scripts need to distinguish
   usage errors (2) from document/runtime failures (1); it also mirrors common
   CLI conventions and clap's own usage exit code.
2. **Print JSON even for usage errors.** Rejected for now: clap owns usage
   output before our code runs; wrapping it would mean intercepting clap
   errors. Documented as a limitation.
3. **Interleave program output and JSON.** Rejected: it makes JSON
   unparseable. Program stdout is embedded as a string instead.
4. **Return values from `run` on stdout in text mode.** Rejected: `run` is the
   batch entry; implicit value printing would pollute program output. `call`
   prints the return value exactly when non-`Unit`.
5. **Human-only messages with numeric exit codes only.** Rejected: diagnostics
   and runtime errors have stable machine codes in both text and JSON.

## Markdown AST shape

The CLI itself does not change the AST. What it exposes:

* `inspect --ast` = canonical `MdNode` tree (see ADR-0005);
* `inspect --units` = `UnitInfo[]` (`marqdo-ir`);
* `inspect --signature` = `{name, local_name, signature: explicit|inferred,
  params:[{name,type,has_default}]}`;
* `inspect --ir` = `Function[]` with `Stmt`/`Expr` and spans;
* `inspect --refs` = link edge array (`spec/references.md` §4);
* `inspect --platform` = `PlatformCapabilities` JSON.

## Compatibility

* Exit codes, stream separation and the documented JSON keys are the public
  CLI contract. Changing them requires this ADR to be superseded and a
  changelog entry.
* The old "design-only starter" README listed planned commands (`add`,
  `lock && sync`, `plugin inspect`); all listed commands except a versioned
  JSON schema are now implemented as described here.
* Diagnostic code renames are breaking for CI; message changes are not.
* No package/ABI/wire changes.

## Security

* `--json`/`--trace` enable effect tracing; traces contain op/module/span/
  duration/result summary, never argument values (no secrets) — see
  `spec/security.md` §4.
* JSON mode embeds program stdout, which is program-controlled data; consumers
  must not treat it as trusted.
* Refusing to run with compile errors happens before host creation, so JSON
  failure output cannot leak effects.
* Strict-mode refusal and capability denials produce stable codes for CI.

## Testing

* CLI behavior is currently covered by manual smoke checks and library-level
  tests; the fixtures workstream is adding CLI integration tests under
  `crates/cli/tests` (goldens/negative cases) and `tests/`.
* `crates/cli/src/args.rs` unit tests cover typed `--arg` parsing and rejection
  of non-constant values.
* `crates/cli/src/output.rs` has no unit tests; diagnostic JSON is exercised
  through check/run smoke tests.
* Add contract tests for: exit codes per failure class, stdout/stderr
  separation, JSON key presence, `call --json` command naming. Runtime errors
  from a non-entry module are covered by
  `runtime_error_in_imported_module_reports_that_module` (ADR-0017).

## Migration & rollout

1. Keep the current shapes stable while adding tests.
2. Add a `schema_version` to JSON envelopes and document the versioned schema;
   then document that unknown keys may be added (forward compatible) while
   existing keys are frozen.
3. Fix the `call --json` `"command":"run"` quirk and JSON errors in
   `add/remove/lock/sync`; both are additive/corrective and should be
   announced in the changelog.
4. If a command is renamed or an exit code changes, update this ADR, the
   README quickstart, `docs/STATUS.md` and CI scripts in the same change.
