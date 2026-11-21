# Analysis v2 and Trace v2 consumers

Both are public, versioned JSON artifacts. A consumer must read them as files,
never link Marqdo's Rust internals or depend on `inspect --ir`.

## Analysis v2 (static)

```bash
marqdo check index.mq.md --emit ./analysis.json
```

- Write atomically; only publish after `check` exits 0. `--refs` is an
  independent Markdown-link check and can make `check` fail; on failure the emit
  target is removed. Do not generate a consumer artifact with
  `check --refs --emit` without checking the exit code.
- `schema: "marqdo.analysis"`, `schema_version: 2`.
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
- Schemas: `spec/schemas/analysis-v2.schema.json`; contract:
  `spec/analysis-v2.md`; consumers: `tests/consumers/analysis_consumer.py`,
  `tests/consumers/validate_protocols.py`.

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
  `return`, `effect`, and possibly `trace.truncated`. Sourced events carry a
  module id and a UTF-8 span.
- Defaults are 100,000 events / 32 MiB; `--trace-max-events` and
  `--trace-max-bytes` adjust them (`0` = unlimited). A consumer must tolerate
  truncation and must not assume the trace is complete.
- Trace records no argument values, secrets or file contents.
- Contract: `spec/trace-v2.md`; schema:
  `spec/schemas/trace-v2.schema.json`; consumer:
  `tests/consumers/trace_consumer.py`.

## Consumer rules

1. Reject unknown schema majors; there is no silent "use latest" fallback.
2. Verify `source.analysis_digest` against the actual Analysis file bytes.
3. Verify `source.root_id`/package identity; do not key on absolute paths.
4. Be deterministic and offline; never execute `.mq.md` yourself, never invent
   host effects, and never inherit Marqdo grants.
5. Report an unsupported/stale artifact explicitly; do not silently downgrade.
