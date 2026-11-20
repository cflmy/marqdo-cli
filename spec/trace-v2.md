# Marqdo Trace v2

Status: **implemented by `marqdo run/call --trace-file <path>` (S2).**
Supersedes Trace v1; v1 was never released and is kept as history.

## 1. Invariants

* Tracing is observation, not execution. Only `run`/`call` produce a trace;
  `check`, `inspect` and `tool list` never execute a document.
* Events come from the interpreter (`call.*`, `branch.eval`,
  `loop.iteration`, `return`) and the host (`effect`). Both share one sequence.
* The writer is atomic; an error run still records its terminal status.
* No argument values, secrets, handles or file contents are recorded.
* Trace capacity is a bounded observation budget. Reaching it truncates the
  trace; it never changes `run.status` or the program’s execution result.
  Execution budgets (`--fuel`, `--timeout`, ...) remain separate and still
  produce `resource.exhausted`.

## 2. Shape

```json
{
  "schema": "marqdo.trace",
  "schema_version": 2,
  "run": {"id": "pid-nanos", "entry": "root:q@0.1.0::index.mq.md::main", "status": "ok"},
  "source": {"root_id": "root:q@0.1.0", "analysis_digest": "sha256:<64 hex>"},
  "limits": {"max_events": 100000, "max_bytes": 33554432},
  "truncated": null,
  "events": [
    {"seq": 0, "kind": "call.enter", "function_id": "root:q@0.1.0::index.mq.md::main",
     "module_id": "root:q@0.1.0::index.mq.md", "span": null, "detail": null},
    {"seq": 1, "kind": "effect", "module_id": "dep:p@0.1.0#<digest>::index.mq.md",
     "span": {"start": 40, "end": 52}, "detail": "ok", "operation": "print",
     "duration_micros": 3}
  ]
}
```

`seq` is globally increasing. `run.status` is `ok` or a stable runtime error
code. Every span-bearing event has a `module_id`; the span is a UTF-8 byte
offset range in that module. `source` only carries project identity and the
Analysis digest; it does not identify the span owner.

`source.root_id` is the Analysis v2 `project.root_id`
(`root:<name>@<version>` or `sha256:<workspace snapshot>`).
`source.analysis_digest` is `sha256:` plus the SHA-256 of the exact Analysis
v2 bytes produced for the same project with the same projection configuration.
A consumer verifies it by hashing the Analysis file.

`truncated` is `null` or:

```json
{"reason": "event_budget", "limit": 20, "written": 20}
```

`reason` is `event_budget` or `byte_budget`. When truncation occurs, the final
event is `{"kind":"trace.truncated","detail":<reason>,"module_id":null,
"span":null,"seq":<one past the last written event>}`.

## 3. Event kinds and span owners

| kind | detail | `module_id` owner |
|---|---|---|
| `call.enter` | null | caller module (entry call: callee) |
| `call.exit` | `ok` / `return` / error code | caller module (entry call: callee) |
| `return` | value type name | current function’s module |
| `branch.eval` | `true` / `false` | current function’s module |
| `loop.iteration` | `continue` / `exit` | current function’s module |
| `effect` | result summary | module from `ExecutionContext` |
| `trace.truncated` | `event_budget` / `byte_budget` | null |

`function_id` is always the canonical callee function id. Future kinds are
additive; consumers must ignore unknown kinds. `effect.operation` names the
`HostOp` (`print`, `fs.read`, ...).

## 4. Capacity

Defaults: 100 000 events and 32 MiB approximate serialized size. `0` means
unlimited for either limit. `run`/`call` expose `--trace-max-events` and
`--trace-max-bytes`; these are options on existing commands, not new commands.
The host stops accepting events once a limit is reached; the writer also
enforces the byte limit on the final serialized document, dropping trailing
events and recording `byte_budget`. Semantic recording is disabled entirely
when no trace was requested.

The `trace` array in `run --json` uses the same budget.

## 5. Staleness

A trace is stale when its `source.analysis_digest` differs from the digest of
the Analysis emitted for the current document. Consumers must not present a
stale trace as current. Moving a project does not change the manifest identity
or digest; changing executable source does.

## 6. Worker transport limits

Plugin worker frames are a 4-byte big-endian length plus JSON, capped at
8 MiB (`8 * 1024 * 1024`). The wire request/result/capability caps are aligned
to `8 MiB - 4096`; error frames remain 1 MiB. A partial header, zero/oversized
length, truncated payload or invalid JSON is a protocol error. A clean EOF is
distinct from a protocol failure. The host marks a worker dead after a transport
failure and never reuses its stdin.

## 7. Compatibility and tests

* Consumers must reject unknown `schema_version` and validate `module_id`
  against Analysis modules.
* JSON Schema: `spec/schemas/trace-v2.schema.json`.
* `tests/consumers/trace_consumer.py` verifies identity, digest and event
  module references.
* CLI tests cover call/effect events, no-secret redaction, error terminal
  status, and event-budget truncation without changing the run status.
