# Analysis and Trace v2 consumers

Both are public, versioned JSON artifacts. A consumer must read them as files,
never link Marqdo's Rust internals or depend on `inspect --ir`.

## Analysis v2 and v3 (static)

```bash
marqdo check index.mq.md --emit ./analysis.json
```

- Write atomically; only publish after `check` exits 0. `--refs` is an
  independent Markdown-link check and can make `check` fail; on failure the emit
  target is removed. Do not generate a consumer artifact with
  `check --refs --emit` without checking the exit code.
- `schema: "marqdo.analysis"`. The writer emits `schema_version: 2` for
  projects without loop transfers and `3` when a function body contains
  `break` or `continue`. Check the version before walking function bodies.
  Both versions encode `else if` in the ordered `branch.arms` array.
- Top level: `producer` (`name`, `version` = product version,
  `markdown_profile`), `project`, `modules`, `diagnostics`.
- `project`: `root_id` (`root:<name>@<version>` for manifest projects),
  `identity_kind`, `lock_digest` (`""` without a lock), `stdlib`
  (`version`, `digest`, `instance_id`; may be null).
- `modules[]`: `id`, `package_instance_id`, `relative_path`, `source_digest`,
  `units`, `functions`, `imports`, `references`, `calls`, `resources`.
  Diagnostics are top-level, not per-module.
- Byte offsets are UTF-8 source offsets. Stable index keys are logical package
  instance ids, never absolute paths.
- `check` does not load plugin libraries, start kernels/providers or execute the
  program.
- Contracts: [Analysis v2](../../spec/analysis-v2.md) and
  [Analysis v3](../../spec/analysis-v3.md). Each links to its schema.
  The independent consumers in `tests/consumers/` check the version and
  decode the public nodes.

View must render the **original Markdown** and use Analysis only to correlate
source offsets/heading units/callable mapping. Knowledge must use heading
anchors, links, resources, package identity and source digests; it must not
assume a `@knowledge` unit has runtime effects. Prove must map only the public
proveable subset and distinguish `proved / disproved / unknown / unsupported`;
a proof certificate needs matching source digest, semantics and contract
identity.

## Trace v2 (dynamic)

```bash
marqdo run index.mq.md --trace-file ./trace.json
```

- `schema: "marqdo.trace"`, `schema_version: 2`.
- `source.analysis_digest` must equal SHA-256 of the **Analysis JSON bytes** for
  the same project/projection configuration. A mismatch means the trace is
  stale; do not bind it to the new source. `run --trace-file` does not emit
  Analysis itself.
- Event kinds: `call.enter`, `call.exit`, `branch.eval`, `loop.iteration`,
  `control.break`, `control.continue`, `return`, `effect` and possibly
  `trace.truncated`. Sourced events carry a
  module id and a UTF-8 span.
- Defaults are 100,000 events / 32 MiB; `--trace-max-events` and
  `--trace-max-bytes` adjust them (`0` = unlimited). A consumer must tolerate
  truncation and must not assume the trace is complete.
- Trace records no argument values, secrets or file contents.
- Contract: `spec/trace-v2.md`; schema:
  `spec/schemas/trace-v2.schema.json`; consumer:
  `tests/consumers/trace_consumer.py`.

## Execution Events v1 (live)

```bash
marqdo run index.mq.md --event-stream jsonl
marqdo call index.mq.md main --arg n=5 --event-stream jsonl
marqdo run index.mq.md --event-stream jsonl --trace-file ./trace.json
```

- `schema: "marqdo.execution-events"`, `schema_version: 1`. This is a separate
  protocol from Analysis/Trace and has its own version axis.
- While active, the CLI stdout is a **pure NDJSON channel**: one JSON object per
  line, `seq` starts at 0 and is strictly increasing. Program stdout/stderr are
  only `stdout.chunk`/`stderr.chunk` events with `encoding: "base64"`,
  `data_base64` and a matching `byte_count`. Concatenated decoded bytes equal a
  normal run's output; invalid UTF-8/NUL/binary bytes are preserved.
- `stream.opened` is always seq 0. A run then has at most one `run.started` and
  exactly one terminal event (`run.completed`, `run.failed`, `run.rejected`) on
  a complete stream. An EOF without a terminal event is `incomplete`
  (SIGKILL/crash/broken pipe); never synthesize success.
- `run.started.source` contains `root_id` and the `sha256:` digest of the exact
  Analysis bytes. `--trace-file` may be combined; both writers share one
  opaque run id, and Trace v2 remains atomic and value-free.
- Structural events: `call.enter`/`call.exit` (`frame_id`, `parent_frame_id`),
  `return`, `branch.eval`, `loop.iteration`, `control.break`,
  `control.continue` and `effect`. Frames identify call
  instances, not functions; a failed argument binding creates no frame.
- Values are off by default (`value_state: not_requested`).
  `--observe-variable NAME` and `--observe-parameter NAME` are exact-name
  selections. `Int`/`Float`/`Bool` are bounded by
  `--observe-max-value-bytes` (default 256); selected `Text` is `redacted` until
  `--observe-text-values` is also passed. Containers, `Memory`, resources and
  plugin handles are `unsupported`; handles are never serialized.
- Structural/value events have a separate budget
  (`--observe-max-events`, `--observe-max-bytes`; defaults 100 000 / 16 MiB).
  Exhaustion emits one `observation.truncated` and still delivers stdout/stderr
  and the terminal event. A broken output pipe fails the run with
  `observe.transport_failed`; it never keeps executing side effects silently.
- `--event-stream` is mutually exclusive with `--json`, rejected by
  `check`/`inspect`/`fmt`/`tool`, and currently rejected by `--mode strict`
  (fail-closed, no downgrade). It is not a capability grant.
- Contract: [`../../spec/execution-events-v1.md`](../../spec/execution-events-v1.md);
  schema: `spec/schemas/execution-events-v1.schema.json`; consumer:
  `tests/consumers/execution_events_consumer.py`; fixtures and Rust gate:
  `tests/consumers/fixtures/execution-events/` and
  `crates/cli/tests/execution_events.rs`.

## Consumer rules

1. Reject unknown schema majors; there is no silent "use latest" fallback.
2. Verify `source.analysis_digest` against the actual Analysis file bytes.
3. Verify `source.root_id`/package identity; do not key on absolute paths.
4. Be deterministic and offline; never execute `.mq.md` yourself, never invent
   host effects, and never inherit Marqdo grants.
5. Report an unsupported/stale artifact explicitly; do not silently downgrade.
