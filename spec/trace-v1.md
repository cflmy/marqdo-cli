# Marqdo Trace v1

Status: **superseded by [Trace v2](trace-v2.md); never released, do not write
v1.** Kept as history.

## 1. Invariants

* Tracing is observation, not execution. Only `run`/`call` can produce a trace;
  `check`, `inspect` and `tool list` never execute a document.
* Events come from the interpreter (`call.*`, `branch.eval`,
  `loop.iteration`, `return`) and the host (`effect`).
* The writer is atomic: a failed run does not leave a partially written JSON
  file; an error run still records the terminal status if execution started.
* No argument values, secrets, plugin handles or file contents are recorded.
  Effect detail is a result summary (`ok` or a stable error code).
* `--trace` keeps its existing human/JSON effect output; `--trace-file` is the
  versioned artifact and does not redefine `--trace`.

## 2. Shape

```json
{
  "schema": "marqdo.trace",
  "schema_version": 1,
  "run": {"id": "pid-nanos", "entry": "root:default::index.mq.md::main", "status": "ok"},
  "source": {"root_id": "sha256:...", "analysis_digest": "sha256:..."},
  "events": [
    {"seq": 0, "kind": "call.enter", "function_id": "root:default::index.mq.md::main", "span": null, "detail": null},
    {"seq": 1, "kind": "effect", "module": "root:default::index.mq.md", "span": {"start": 40, "end": 52}, "detail": "ok", "operation": "print", "duration_micros": 3},
    {"seq": 2, "kind": "call.exit", "function_id": "root:default::index.mq.md::main", "span": null, "detail": "ok"}
  ]
}
```

`seq` is globally increasing within one run (semantic and effect events share
the counter). `run.status` is `ok` or a stable runtime error code. Span fields
use UTF-8 byte offsets into the source identified by `source.root_id` and
`source.analysis_digest`.

## 3. Event kinds

| kind | detail | Meaning |
|---|---|---|
| `call.enter` | null | function entry; `function_id` is canonical |
| `call.exit` | `ok` / `return` / error code | function exit |
| `return` | value type name | explicit `return` statement; no value content |
| `branch.eval` | `true` / `false` | `if` condition for an `if/else/end` group |
| `loop.iteration` | `continue` / `exit` | `while` condition for a `while/end` group |
| `effect` | result summary | a `HostOp` effect; `operation` names it |

Future event kinds are additive. Consumers must ignore unknown kinds rather
than guessing their semantics.

## 4. Staleness

`source.analysis_digest` is the digest of the Analysis v1 projection built
from the same loaded project. If a consumer compares it with a newer analysis
and the digest differs, the trace is `stale` and must not be presented as the
current document's execution.

## 5. Privacy and limits

* Values and secrets are never serialized into a trace.
* Foreign/plugin effects record only the operation and success/error summary.
* A trace file can be large for long loops; the current implementation has no
  event-count cap, but consumers must stream/limit. A future version may add a
  bounded mode.
* A failed/aborted run records the status it reached; it is never marked
  successful.

## 6. Not included

Interactive breakpoints, a debug server, time travel, notebook sessions,
WebSocket streaming and full argument-value capture are out of scope for v1.
