# Execution Events v1

Status: **implemented by `marqdo run/call --event-stream jsonl`** (P0 and P1 of
RFC-OBS-0001). It is an independent public protocol: it is not Analysis v2,
not Trace v2 and not Tool Protocol v1. The decision record is
[ADR-0059](../docs/adr/0059-runtime-observability-and-execution-events.md); the
design input is [RFC-OBS-0001](../docs/architecture/RFC-OBS-0001-runtime-observability.md).

## 1. Transport and negotiation

* Media type: `application/x-ndjson` (JSON Lines). One complete UTF-8 JSON
  object per line, terminated by `\n`; no unescaped newline occurs inside an
  event.
* The stream uses the CLI stdout as a **dedicated machine channel**. While
  `--event-stream jsonl` is active, the program's `stdout` and `stderr` are
  carried only as `stdout.chunk` / `stderr.chunk` events and are not written
  raw to stdout. Human diagnostics may still go to OS stderr.
* `--event-stream jsonl` is available only on `run` and `call`. It is
  mutually exclusive with `--json`; `check`, `inspect`, `fmt`, `tool` and the
  other static commands reject the option. `--trace-file` may be combined.
* `--mode strict` currently rejects `--event-stream` with a structured
  `run.rejected` (`strict.unsupported_grant`); there is no silent downgrade to
  `project`.
* The consumer checks the negotiated protocol from `stream.opened`
  (`schema` + `schema_version`). Consumers **must reject unknown major
  versions**, and must not silently treat another major as v1.
* A consumer that cannot read the stream (slow, closed pipe, killed process)
  may see a truncated final line or no terminal event. It must report
  `incomplete`; it must never synthesize a successful terminal event.

## 2. Envelope

Every normal line has exactly these top-level fields:

```json
{
  "schema": "marqdo.execution-events",
  "schema_version": 1,
  "run_id": "12345-1700000000000000000",
  "seq": 2,
  "kind": "call.enter",
  "payload": {}
}
```

| Field | Type | Meaning |
|---|---|---|
| `schema` | string | Always `marqdo.execution-events`. |
| `schema_version` | integer | Major version; currently `1`. |
| `run_id` | string | Opaque identity of this one run. Never an OS PID used as a cross-run identity. Different for every invocation, including repeated runs in one process. |
| `seq` | integer | Global, strictly increasing, starts at 0, assigned by one dispatcher. `seq` is the event sequence, not a frame or iteration identity. |
| `kind` | string | Stable namespaced event name. New kinds are additive; consumers ignore unknown kinds. |
| `payload` | object | Kind-specific object defined below. Unknown optional fields are ignored. |

Event ordering is the order of admission to the single dispatcher. Independent
OS streams (for example a foreign subprocess writing its own fd) do not have a
portable physical ordering and are not part of this protocol.

## 3. Lifecycle and event kinds

Exactly one `stream.opened` is written when the JSONL writer is initialized,
before project preparation. At most one `run.started` is written. On success or
failure there is exactly one terminal event, and no event is written after it.
The normal order is:

```text
stream.opened (seq 0)
  -> run.rejected                         (prepare/authorization/compile failure)
  or run.started -> ... -> run.completed | run.failed
```

`run.completed`, `run.failed` and `run.rejected` are the only terminal events.
`SIGKILL`, a crash or a broken pipe may leave the stream without a terminal
event; that is a legitimate `incomplete` boundary.

### 3.1 `stream.opened` (base, seq 0)

Required payload fields:

```json
{
  "producer": {"name": "marqdo", "version": "3.0.0-beta.1"},
  "protocol": {"name": "marqdo.execution-events", "schema_version": 1},
  "observation": {
    "level": "structural",
    "values": false,
    "selected_variables": [],
    "selected_parameters": [],
    "allow_text_values": false
  },
  "limits": {
    "max_observation_events": 100000,
    "max_observation_bytes": 16777216,
    "max_value_bytes": 256
  }
}
```

`observation.level` is `base`, `structural` or `values`. It reports what the
producer is willing to publish, not a security grant. `stream.opened` does not
carry a source identity because no project has been prepared yet.

### 3.2 `run.started` (base)

Required payload fields: `entry`, `mode`, `source`; optional
`started_unix_ms` for diagnostics only.

```json
{
  "entry": "root:demo@0.1.0::index.mq.md::main",
  "mode": "project",
  "source": {
    "root_id": "root:demo@0.1.0",
    "analysis_digest": "sha256:<64 lowercase hex>"
  },
  "started_unix_ms": 1763800000000
}
```

`source.analysis_digest` is `sha256:` plus the SHA-256 of the **exact bytes**
of the Analysis v2 file produced for the same project and projection
configuration (not a re-serialization of the in-memory IR). If the source
changes, existing events become stale; consumers must not render them onto the
new source.

### 3.3 `stdout.chunk` / `stderr.chunk` (base)

Required payload fields:

```json
{"encoding": "base64", "data_base64": "5byA5aeL5paH5pysCg==", "byte_count": 13}
```

* `encoding` is `base64` (standard alphabet, padded).
* `data_base64` is the byte-exact data. Invalid UTF-8, NUL, binary bytes and
  split multi-byte characters are preserved.
* `byte_count` equals the decoded byte length. A consumer must verify it.
* Chunks are bounded (implementation maximum 64 KiB before base64 encoding);
  a larger host write is split into consecutive chunks in order.
* These events are emitted at the host's real commit point: bytes are exactly
  those accepted by the host output budget. A rejected write emits nothing.
* All committed chunk events precede the terminal event. Concatenating the
  decoded bytes must equal the bytes a normal run writes to stdout/stderr for
  the same arguments and grants.

### 3.4 Structural events (structural)

| Kind | Required payload fields |
|---|---|
| `call.enter` | `frame_id`, `parent_frame_id`, `function_id`, `module_id`, `span` |
| `call.exit` | `frame_id`, `status`, `module_id`, `span` |
| `return` | `frame_id`, `value_type`, `module_id`, `span` |
| `branch.eval` | `frame_id`, `result`, `module_id`, `span` |
| `loop.iteration` | `frame_id`, `decision`, `module_id`, `span` |
| `effect` | `operation`, `result_code`, `module_id`, `span` |

* `frame_id` is an opaque run-unique string (`f1`, `f2`, ...). It identifies
  one call instance, not a function and not a source location.
* `parent_frame_id` is `null` for the entry frame, otherwise the direct
  caller's `frame_id`.
* A frame is announced by `call.enter` only **after** argument binding
  succeeded. A binding failure produces no frame. Every entered frame that is
  not cut off by transport loss/truncation gets exactly one `call.exit`;
  `return` (when present) precedes `call.exit`.
* `call.exit.status` is `ok`, `return`, or a stable runtime error code.
* `branch.eval.result` is a JSON boolean. `loop.iteration.decision` is
  `continue` (body will execute) or `exit` (condition was false). The event is
  emitted once per condition evaluation; iteration indices are not part of v1.
* `effect.result_code` is a non-sensitive result summary (for example `ok` or a
  stable error code). It never carries effect arguments or file contents.
* `module_id` is a logical package module id; `span` is
  `{"start": <byte>, "end": <byte>}` in UTF-8 source offsets, or `null`. A
  span must belong to the declared module.

Example:

```json
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":3,"kind":"call.enter","payload":{"frame_id":"f1","parent_frame_id":null,"function_id":"root:demo@0.1.0::index.mq.md::main","module_id":"root:demo@0.1.0::index.mq.md","span":null}}
```

### 3.5 Value observation events (values)

`variable.assigned` required payload fields: `frame_id`, `name`,
`value_type`, `module_id`, `span`, `value_state`; optional `value` when
`value_state` is `observed` or `truncated`.

`parameter.observed` required payload fields: `frame_id`, `name`, `source`,
`value_type`, `module_id`, `span`, `value_state`; optional `value`. `source`
is `provided` or `default` (a source default is a real runtime value but not a
user-provided argument).

`value_state` is one of:

| State | Meaning |
|---|---|
| `not_requested` | The name was not selected for observation; no value is encoded. |
| `redacted` | The name was selected but policy hides the value (for example `Text` without `--observe-text-values`). |
| `truncated` | The value was encoded but exceeded `max_value_bytes`; `value` is the safe prefix. |
| `unsupported` | The value type is not a v1 scalar; only its type is reported. |
| `observed` | The scalar value is encoded normally. |

Value encoding reuses the `marqdo.value.v1` wire representation of the value:
`{"type":"int","decimal":"42"}`, `{"type":"float","value":1.5}`,
`{"type":"bool","value":true}`, `{"type":"text","value":"..."}`. For a
truncated text the object also carries `"truncated": true` and
`"original_byte_count": <n>`. Text truncation never splits a UTF-8 scalar.
Lists, maps, tables, formulas, code, memory, resources and plugin handles are
`unsupported`; they are never recursed into and handles are never serialized.
A text value is never published without the explicit
`--observe-text-values` consent, even when the name was selected. A
`--observe-max-value-bytes` value of `0` means unlimited; otherwise text is
cut only at a UTF-8 scalar boundary and marked `truncated`.

`variable.assigned` is emitted only after the assignment was actually
committed. It never re-evaluates the right-hand side and never back-fills the
final value of a variable into an earlier assignment.

### 3.6 `observation.truncated` (any)

Required payload fields: `reason`, `limit`, `written`, `dropped`. `reason` is
`event_budget` or `byte_budget`. It is emitted at most once per run, only when
the structural/value observation budget is exhausted. It does not stop
stdout/stderr events or the terminal event. `dropped` may be `null` when the
producer cannot know the exact count.

### 3.7 Terminal events (base)

`run.completed`:

```json
{"status":"ok","value":{"type":"int","decimal":"10"}}
```

`run.failed`:

```json
{"status":"error","error":{"code":"runtime.type","message":"safe diagnostic message","module_id":"root:demo@0.1.0::index.mq.md","span":{"start":120,"end":141}}}
```

`run.rejected`:

```json
{"status":"rejected","phase":"prepare","error":{"code":"frontmatter.invalid","message":"safe diagnostic message"}}
```

* `run.completed.value` uses the same wire encoding as `run/call --json`.
* `run.failed.error.code` reuses the existing stable runtime error family; the
  message is the same safe diagnostic text already printed to humans. It does
  not add argument values, secrets, file contents or memory bytes.
* `run.rejected.phase` is `options`, `prepare`, `compile`, `authorization` or
  `strict`. It distinguishes failures before execution started.
* A terminal event is written only after all committed chunk events. A
  `run.completed` does not claim the consumer has read or displayed the bytes.

## 4. Budgets, backpressure and failure

Three separate resources exist and are never conflated:

1. **Execution budgets** (`--fuel`, `--timeout`, `--max-output`, memory
   budgets). Observation never widens them. A rejected host output write emits
   no chunk event.
2. **Observable structural/value budget.** Defaults are 100 000 events and
   16 MiB, adjustable with `--observe-max-events` / `--observe-max-bytes`
   (`0` = unlimited). Exhaustion truncates only these events and emits
   `observation.truncated`.
3. **Critical transport events.** `stream.opened`, `run.started`, stdout/stderr
   chunks and the terminal event are not dropped by the structural budget.

Writes are synchronous and bounded. A slow consumer exerts IO backpressure on
the run; the CLI must not accumulate an unbounded queue or spawn an unbounded
event thread. If the JSONL writer fails (closed pipe, write error), the run
must fail with `observe.transport_failed`, return a non-zero exit status and
stop a program that would otherwise keep producing side effects. The CLI must
not write a partial JSON line after a failure. Side effects that already
happened are not rolled back and must not be claimed as rolled back.

If event transport is truncated but still writable (structural budget), the
program's language behavior, return value, output bytes and capability
authorization are unchanged.

## 5. Value policy and security

* Event mode is not a capability grant and cannot enable network, filesystem,
  process, foreign, provider or plugin access. `pure`, `project` and `strict`
  keep their documented policy; if a project extension is rejected in a mode,
  event mode is rejected for the same reason.
* Observation is off by default. Type/status metadata is the default; raw
  values require an explicit exact-name selection. Text requires a second
  explicit consent. Selection is not automatic from a manifest, View setting
  or environment.
* Program stdout/stderr may contain secrets by the program's own intent; the
  protocol preserves bytes and does not promise automatic redaction of program
  output.
* Trace v2 and Analysis v2 never gain argument/local values, handles, memory
  bytes or new secret-bearing fields through this protocol.
* A third-party extension's raw OS stdout/stderr must not pollute the JSONL
  channel. Such output remains external process data: it is either captured by
  the controlled host transport or the combination is unsupported/fail-closed.
  In particular, an in-process **native plugin** can write fd 1 directly, so an
  event-mode run that selects one is rejected with
  `ext.event_stream_unsupported_native_plugin` before loading; use a worker
  plugin or disable event mode.
* Tests use synthetic sentinel values and assert that unselected values never
  appear in the stream, Trace or `--json`.

## 6. Relation to Analysis v2 and Trace v2

| Artifact | Relation |
|---|---|
| Analysis v2 | Static only. `run.started.source` binds to its exact bytes via `analysis_digest`. Events never replace it. |
| Trace v2 | Unchanged schema/version and privacy promise. In a combined run both writers share the CLI-generated `run_id`; Trace v2 gains no frame/value fields and is still published atomically after the run. |
| Tool Protocol v1 | Unchanged. A consumer invokes the released binary via `MARQDO_BIN` and negotiates by reading `stream.opened`; `tool.json` does not implicitly advertise an `events` field. |
| View SSE | Implemented by the View repository by adapting this JSONL stream. The CLI implements no HTTP/SSE. |

## 7. Versioning and compatibility

* Independent major version `1`, not tied to the product version or Trace v2.
* Additive optional fields and new event kinds are allowed in v1. Changing a
  required field's meaning, ordering guarantees, redaction promise, source
  digest definition or making private data public by default requires a new
  major.
* Consumers must ignore unknown `kind` and unknown optional fields, and must
  reject unknown `schema_version` majors.
* `--help`, `docs/developers/analysis-trace.md`,
  `docs/developers/compatibility.md` and `COMPATIBILITY.md` list the protocol
  and its exact option names.

## 8. Test fixtures and consumers

* Schema: [`schemas/execution-events-v1.schema.json`](schemas/execution-events-v1.schema.json).
* Independent consumer: `tests/consumers/execution_events_consumer.py`.
* Fixtures: `tests/consumers/fixtures/execution-events/`.
* Rust gate: `crates/cli/tests/execution_events.rs` runs the real binary and
  checks envelope, sequence, base64 byte fidelity, terminal behavior,
  redaction, frames, traces and output purity.
