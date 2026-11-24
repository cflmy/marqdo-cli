# Marqdo Observation

`run` and `call` execute once through the interpreter and capability Host.
The Host emits typed observations at execution boundaries. The CLI assigns
one run ID and one sequence and serializes each public event once.

```sh
marqdo call index.mq.md calculate --arg n=3 --output jsonl
marqdo call index.mq.md calculate --arg n=3 --output json
marqdo call index.mq.md calculate --arg n=3 --output jsonl --observation-file run.json
marqdo check index.mq.md --emit analysis.json
```

Text is the default output. JSONL flushes each event to stdout as it occurs.
JSON writes one snapshot to stdout after the run. `--observation-file`
atomically publishes a snapshot through a private temporary file in the
destination directory. On Unix the published file is private to the caller.
Every destination in one invocation receives the same events. Diagnostics
use OS stderr. Captured program stdout and stderr are byte events, with
standard padded base64 and a decoded byte count. Both may contain sensitive
program output. Machine destinations refuse in-process native plugins.
Strict mode supports text and JSON stdout through its isolation supervisor.
Live and file destinations are rejected in strict mode.

## Event contract

Each object has `schema: "marqdo.observation"`, `schema_version: 1`, an
opaque `run_id`, zero-based contiguous `seq`, `kind` and `payload`. See
the [event schema](schemas/observation-event.schema.json) for the exact
fields. Reject other format identifiers, unknown kinds and invalid payloads.

- `stream.opened` is first, with producer, capture policy and budget limits.
  The capture level is `base` (lifecycle only), `structural` (frame, branch,
  loop and effect events) or `values` (explicitly selected names); the
  snapshot `capture` metadata repeats the same level.
- `run.started` binds an entry, mode and source. `source.root_id` and
  `source.analysis_digest` identify the exact Analysis JSON emitted by
  `check --emit` for the same project and static options. The digest is
  SHA-256 over the file bytes, including formatting. Before highlighting a
  source span, validate the binding, module source digest and original UTF-8
  half-open byte range. A changed source or Analysis invalidates attribution.
- `call.enter` identifies the call instance and its parent. `call.exit`
  closes that same instance. Call carriers belong to the caller module.
  Parameter spans identify declarations in the called module.
- `branch.eval`, `loop.iteration`, `control.break`, `control.continue`,
  `return` and `effect` record actual execution in order. An effect
  includes its operation, result code and source location where available.
- `parameter.observed` and `variable.assigned` carry names, types and
  source locations. Values are omitted by default. Select exact names
  with `--observe-parameter` and `--observe-variable`. Selected Text also
  requires `--observe-text-values`. The same consent applies to file and
  stream. Only supported scalars are recorded. Memory, resources and plugin
  handles are never included.
- `observation.truncated` reports the first exhausted observation budget.
  Structural and selected-value events may stop at that point. Lifecycle
  and committed byte chunks remain subject to their own execution and
  output limits. The interpreter continues under unchanged program limits.
- Error locations in `run.failed` carry `module_id`, `relative_path`, `line` and
`column`; a snapshot never embeds an absolute host path. `run.completed`
contains the explicit return using [Value Wire](value-wire.md).
  A result containing a non-transferable handle has a null wire value and
  identifies its type. An explicit result, like a program print, can itself
  contain sensitive data. `run.failed` follows a started run and contains
  the error. `run.rejected` reports failure before start. A complete run
  has exactly one terminal event, last. EOF or interrupted delivery without
  a terminal is incomplete.

## Snapshot contract

The [snapshot schema](schemas/observation-file.schema.json) describes a
JSON object with the same format identifier, `run` metadata, `capture`
metadata, and `events` validated against the event schema. The `run.id`
matches every event, `run.source` matches the start event or is null,
and `run.status` agrees with the terminal. `capture.truncated` matches
the truncation event or is null. A supervisor that cannot obtain complete
child output may report an incomplete stdout snapshot without a terminal.
An interrupted file write does not replace the previously published file.
EOF during an interrupted stream can leave a partial final line.

Validate schema, sequence, lifecycle and source binding before using
the artifact. A structurally truncated run has a terminal but lacks some
execution events. An incomplete delivery provides no evidence about
the final execution outcome.
