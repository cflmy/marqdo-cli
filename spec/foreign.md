# Marqdo Foreign Kernels v1

Status: **one-shot kernels for Python and Node/JavaScript**, frozen protocol for
M0.5. There are no sessions or persistent kernels.

Implementation: `crates/marqdo-foreign/src/lib.rs` (process supervision and
protocol), `crates/marqdo-host/src/kernels.rs::foreign` (capability check and
Marqdo mapping), runtime tests `crates/marqdo-foreign/tests/kernels.rs`,
example `examples/python-foreign.mq.md`.

## 1. A code fence is a value, not a process

A `code_block` node becomes an inert `Code { language, source, content_hash,
unit }` resource (`crates/marqdo-frontend/src/resources.rs`,
`marqdo_value::CodeValue`). Reading, checking, rendering or importing a
document never starts a foreign process. A process is created only when
`foreign.run` or `foreign.call` is executed after the capability check.

Access is by `ref("#heading")` (exactly one code/formula resource in that
heading range; `ref.ambiguous` otherwise — see `spec/references.md` §5):

````markdown
## Python 校验

```python
import json, math, sys
envelope = json.load(sys.stdin)
r = int(envelope["input"]["entries"]["r"]["decimal"])
print(json.dumps({"type": "float", "value": math.pi * r ** 2}))
```

## main

**结果 = foreign.call(ref("#python-校验"), {"r": 3})**

> 打印(结果)
````

The anchor is the GitHub-style slug of the heading (`#python-校验`), not the
heading text.

## 2. Supported languages

`normalize_language` is ASCII case-insensitive and trims surrounding
whitespace:

| Canonical | Accepted aliases | Interpreter lookup |
|---|---|---|
| `python` | `python`, `python3`, `py` | `python3`, then `python` on `PATH` |
| `javascript` | `javascript`, `js`, `node`, `nodejs` | `node` on `PATH` |

Any other language yields `ForeignError::KernelUnavailable` (the `Code` value is
still valid data). `detect(language)` returns `KernelInfo { language, program,
version }` by scanning `PATH` and running the interpreter with `--version`
(5 s timeout, 64 KiB output cap, cleared environment except `PATH`); the
`version` is the first non-empty line of stdout or stderr. `detect` and
execution use the same `PATH`-only environment.

## 3. `RunRequest` / `RunResult`

```rust
pub struct RunRequest {
    pub language: String,
    pub source: String,
    pub input: Option<serde_json::Value>, // an already-typed wire value
    pub timeout: std::time::Duration,
    pub max_output_bytes: usize,
}

pub struct RunResult {
    pub status: RunStatus,          // Success | Failed | TimedOut
    pub stdout: String,             // lossy UTF-8 for `run`
    pub stderr: String,             // preserved on failure
    pub exit_code: Option<i32>,     // None on signal/timeout
    pub duration: std::time::Duration,
    pub value: Option<Value>,       // only `call` on success
}
```

`RunRequest::new` defaults to a 30 s timeout and 1 MiB output cap **for direct
library use**. The CLI host constructs requests with `StdHost` defaults:
`foreign_timeout = 10 s`, `foreign_output_limit = 1 MiB` (stdout+stderr
combined). The CLI has no per-kernel flag for these; `--timeout` only sets the
interpreter wall-clock deadline.

`RunStatus::as_str()` yields `success`, `failed`, `timed_out`.

## 4. stdin envelope

The program is executed as a file; stdin receives exactly one JSON object
(`crates/marqdo-foreign/src/lib.rs::request_envelope`):

```json
{
  "wire_version": 1,
  "kind": "request",
  "input": <type-tagged wire value, or null>
}
```

`input` is **already encoded with the type-tagged value format**
(`spec/value-wire.md`), not a plain JSON mirror of the Marqdo value. For
example `{"r": 3}` becomes
`{"type":"map","entries":{"r":{"type":"int","decimal":"3"}}}`. The example
kernel reads `envelope["input"]["entries"]["r"]["decimal"]` accordingly. When
`input` is absent, the field is JSON `null` and the write side closes stdin
right after writing (EOF).

Marqdo values that cannot cross the wire (none in v1, but see
`spec/value-wire.md` limits) are encoded before the process starts; the
envelope may be size-limited by the host's output/wire budgets.

## 5. `run` versus `call`

| | `foreign.run(ref, input?)` | `foreign.call(ref, input)` |
|---|---|---|
| One-shot isolated execution | yes | yes |
| stdin envelope | yes (input optional) | yes (input required) |
| stdout contract | captured as data; any content | must be **exactly one** `wire_version` 1 typed value; no logs/trailing noise |
| result | `Map` with `status`, `stdout`, `stderr`, `exit_code`, `duration_ms`, `value` (always `None` for `run`) | decoded `Value` |
| non-zero exit | `status: "failed"` in the returned map | `Ok(RunResult { status: Failed })` from the library, but the host turns it into `foreign.failure` runtime error |
| timeout | library returns `Ok` with `status: "timed_out"`; host returns the map | library returns `ForeignError::Timeout`; host maps to `resource.exhausted` |
| stdout pollution / invalid wire | irrelevant | `ForeignError::Protocol` → `foreign.failure` |

`call` parses stdout with `serde_json` and then `marqdo_value::from_wire` with
default limits; an unknown `type` tag, bad decimal, JSON syntax error, invalid
UTF-8 or trailing bytes are all protocol errors. A successful call whose stdout
has no typed value is a protocol error at the host boundary ("foreign call
returned no typed value" / `foreign.failure`).

The host result map from `foreign.run` (`crates/marqdo-host/src/kernels.rs::
foreign_result_map`) is:

```text
{status: "success"|"failed"|"timed_out",
 stdout: Text, stderr: Text, exit_code: Int|None,
 duration_ms: Int, value: None}
```

## 6. Process model and limits

`crates/marqdo-foreign/src/lib.rs`:

1. normalize the language (unknown → `KernelUnavailable` with install hint);
2. find the interpreter on `PATH`;
3. create a **private unique temp working directory** (mode 0700 on Unix) and
   write the program to a private temp file (`.py`/`.js`, mode 0600);
4. spawn the interpreter **directly with `argv`** (no shell, no string
   interpolation), cwd = the fresh temp dir, environment cleared except
   `PATH`, stdin/stdout/stderr piped;
5. a dedicated thread writes the stdin envelope and closes the pipe; reader
   threads capture stdout and stderr against a **shared** `max_output_bytes`
   budget; the supervisor polls `try_wait` and kills the child on the wall-clock
   deadline or output overflow;
6. after the child is reaped, readers get a 2 s grace to finish; cleanup (Drop)
   removes the temp file and directory on success and error paths.

Bounds: wall-clock `timeout` (10 s in the CLI), combined output
`max_output_bytes` (1 MiB in the CLI; stderr counts too), reader grace 2 s,
detection timeout 5 s / 64 KiB. A kernel that ignores stdin and exits does not
hang the host. `OutputTooLarge` drops the captured output and kills the child.
Grandchild processes are not killed when the direct child is killed.

Environment: only `PATH` is inherited, so Marqdo secrets in other environment
variables do not leak by default. This is hygiene, not a security boundary.

## 7. Capability gating

`foreign.run`/`foreign.call` are effectful builtins
(`crates/marqdo-ir/src/builtins.rs`). The host adapter
(`crates/marqdo-host/src/kernels.rs::foreign`) first checks
`Capabilities::check_foreign`, which requires both `foreign` and `process`.

* default (`Capabilities::pure()`/`project()`): denied →
  `permission.denied` "foreign kernel execution was not granted";
* `--allow-foreign` in `project` mode grants both flags;
* `pure` mode rejects `--allow-foreign` at CLI parse time (exit 2);
* `strict` mode refuses to run before any kernel is considered;
* a manifest `capabilities.process`/foreign request never grants anything;
* the check is repeated for every call (not cached at parse time).

If the first argument is not a `Code` value, the host returns `runtime.type`
("foreign kernel expects a Code value from `ref("#代码块")`"). A code fence with
an empty language is `kernel.unavailable` ("no kernel can be selected").
Unsupported languages are `kernel.unavailable` with an install hint.

## 8. Error mapping

| Library error | Host runtime code |
|---|---|
| `KernelUnavailable { language, hint }` | `kernel.unavailable` (message includes hint) |
| `Spawn { message }` | `host.error` |
| `Io { message }` | `host.error` |
| `Timeout { seconds }` | `resource.exhausted` |
| `Protocol { message }` | `foreign.failure` |
| `OutputTooLarge { limit }` | `resource.exhausted` |

`foreign.call` with a non-zero exit becomes `foreign.failure` with the stderr
text preserved; a successful process whose stdout is not a typed value is also
`foreign.failure`.

## 9. Security caveats

* **Process isolation is not a sandbox.** The child runs as the host user and
  can read/write any file the user can, open sockets, spawn processes and
  persist beyond the one-shot execution. Fresh cwd, cleared environment and a
  killed direct child are limited hygiene measures. Only `strict` mode
  (currently refused) would promise OS-level confinement.
* There is no network policy for foreign kernels; `net` capability applies to
  host operations, not to what a child process does.
* Output is captured, not inherited; the child cannot write to the CLI's
  stdout/stderr directly, but it can still affect the filesystem.
* Kernel availability is environment-dependent (`python3`/`python`/`node` on
  `PATH`). Missing kernels must be fixable by the user (the error includes an
  install hint).
* `foreign.run` returns raw stdout as data; consumers must not treat it as a
  typed value unless they parse it themselves. `foreign.call` is the typed
  path.
* Sessions, shared state, persistent kernels and automatic environment
  management (uv/venv) are not implemented. Python/Node are optional foreign
  kernels, never the official execution backend.
