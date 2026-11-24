# ADR-0059: Runtime observability and Execution Events v1

> Historical. [ADR-0068](0068-single-current-public-contracts.md) supersedes
> the public contracts described in this record.

- **Status:** Accepted
- **Date:** 2026-11-22
- **Decision owners:** CLI, runtime and host maintainers
- **Inputs:** RFC-OBS-0001 (issue `#1` of `cflmy/marqdo-cli`), Trace v2,
  Analysis v2 and Tool Protocol v1. Those documents were retired in place by
  [ADR-0068](0068-single-current-public-contracts.md) and are available at
  their tagged commits, not in the current `spec/` tree.

## Context

Marqdo has two public, versioned machine-readable artifacts: Analysis v2
(static, never executes) and Trace v2 (one complete document published
atomically after a run). Neither can stream program stdout/stderr while a task
is still running, and Trace v2 deliberately has no argument or local-variable
values.

External tools (`marqdo-view`, agents, IDEs, Knowledge/Prove) therefore had two
bad options: parse human terminal output, or embed/link private runtime types
and build a second execution path. Both break the repository boundary this
project exists to maintain: **the CLI is the only runtime**.

## Decision

1. **A third, independent public protocol exists: Execution Events v1.**
   * `schema = "marqdo.execution-events"`, `schema_version = 1`.
   * It is a one-way NDJSON (JSON Lines) stream over the CLI's protocol stdout.
   * It is **not** Trace v2, not Analysis v2 and not Tool Protocol v1. Each has
     its own version axis. Adding optional events/fields is allowed in v1;
     changing required field meaning, ordering, redaction promises or the
     source-digest definition requires a new major.
   * The CLI implements no HTTP, SSE, WebSocket, browser session or
     View/Agent/IDE-specific branch. Consumers convert the stream themselves.

2. **The stream is opt-in and never changes semantics.**
   `marqdo run/call --event-stream jsonl` turns the CLI stdout into a pure
   JSONL channel. Without the option, stdout, stderr, exit codes, Trace and
   Analysis behavior are byte-for-byte unchanged. Observation is not a
   capability grant and cannot widen `pure/project/strict` authorization.

3. **`--event-stream jsonl` and `--json` are mutually exclusive.**
   `--trace-file` may be combined; Trace v2 keeps its atomic, post-run,
   low-sensitivity contract. Program stdout/stderr are encapsulated as
   `stdout.chunk`/`stderr.chunk` events and are never written raw to the
   protocol stdout.

4. **Events are emitted at real commit points, not from final buffers.**
   Stdout/stderr chunks originate in `StdHost::resource_write` immediately
   after the host accepts and commits bytes, inside the same budget path. The
   final `take_output()` buffer is never split into fake "live" chunks.
   Selected local-variable events originate after the assignment has been
   committed to `Frame.locals`; a rejected memory write never produces a
   success event.

5. **The host owns the observation facade.**
   `marqdo-host::observe` defines typed internal events (`ObservationEvent`),
   the `ObservationSink` trait, `ObservationConfig`, frame identity
   (`FrameId`) and the observation budget. The runtime emits borrowed events
   through default `Host` methods; it never hands the sink `Frame.locals`,
   the whole `MemoryStore`, or raw OS handles. The CLI owns only JSONL
   encoding and the public envelope. No new crate is introduced.

6. **Call instances get an opaque identity.**
   Each entered frame gets a run-unique `frame_id` (`f1`, `f2`, ...) and a
   `parent_frame_id` (`null` at the top level). Frame IDs exist only in
   Execution Events v1; Trace v2 is not given a new field. Argument binding
   happens before a frame is announced, so a failed binding never produces a
   dangling frame.

7. **Values are selected, bounded and off by default.**
   * Default event mode publishes types/status only (`value_state:
     not_requested`).
   * `--observe-variable NAME` / `--observe-parameter NAME` are exact-name,
     repeatable selections. There is no wildcard, regex, path expression or
     `--dump-all-locals`.
   * Supported value types are the scalars `Int`, `Float`, `Bool`; `Text`
     additionally requires `--observe-text-values`. Lists, maps, tables,
     formulas, code, memory, resources and plugin handles report
     `unsupported` and are never expanded or serialized by `Debug`.
   * Every value is capped by `--observe-max-value-bytes` (default 256) and
     reports `not_requested`, `redacted`, `truncated`, `unsupported` or
     `observed` distinctly. A text truncation never splits a UTF-8 scalar.
   * Observation lowers confidentiality: it is an explicit developer action.
     It is never enabled from a project manifest, never recorded in Trace v2
     or Analysis v2, and never automatically enabled by View.

8. **Budgets, ordering and failure are defined.**
   * One dispatcher assigns `seq` from 0; stdout/stderr, structural and value
     events share the observable order of admission.
   * Structural/value events have a bounded budget (default 100 000 events /
     16 MiB). Exhaustion emits one `observation.truncated` and stops only
     non-critical events; stdout/stderr and the terminal event still flow.
   * `stream.opened` (seq 0), `run.started`, stdout/stderr and the terminal
     `run.completed`/`run.failed`/`run.rejected` are not dropped by the
     structural budget.
   * Writes are bounded and synchronous: a slow consumer applies IO
     backpressure instead of growing an unbounded queue. A write failure
     aborts the run with `observe.transport_failed`; the CLI must not keep
     executing a program with side effects after its event pipe broke, and it
     must not write a half JSON line.
   * A killed/crashed process may have no terminal event. Consumers must
     report `incomplete`, never synthesize success.

9. **`run.rejected` covers pre-execution failure in event mode.**
   After the writer is open, prepare/authorization/compile failures produce a
   structured terminal `run.rejected` with the existing stable error code and
   phase, a non-zero exit, and no `run.started`. CLI option parsing errors
   still use the ordinary clap stderr path before the protocol exists.

10. **The source binding is the Analysis digest.**
    `run.started.payload.source` carries `root_id` and
    `analysis_digest` = `sha256:` plus the SHA-256 of the exact Analysis v2
    bytes for the same project/projection configuration. A changed source
    makes old events stale; consumers must not render them onto new source.

11. **The Trace v2 run identity is shared, without changing Trace v2.**
    In a combined run the CLI composition root creates one opaque `run_id` and
    passes it to both writers, so `trace.run.id == execution stream run_id`.
    Trace v2 keeps `schema_version: 2` and gains no frame/value fields.

## Consequences

- External tools can consume live output with a released binary and a
  documented schema; they still need no private Rust crate.
- The runtime gains observation hooks at existing commit points. The default
  (no option) path installs no sink and does not clone `Value`s or serialize
  observation data.
- The value policy is intentionally smaller than a debugger. Breakpoints,
  stepping, time travel, raw memory snapshots, memory-byte events and a
  bidirectional Debug Protocol remain a separate future RFC.
- Memory metadata events (`memory.region.*`, `memory.budget`) are P2 in
  RFC-OBS-0001 and are **not** part of this decision's implemented scope; no
  raw memory content or handle is ever exposed.

## Rejected alternatives

- **Stream Trace v2 while running.** Breaks existing complete-file consumers
  and cannot naturally carry live stdout/stderr or reject-before-run events.
- **Make events a flag of `--json`.** Mixes a complete result envelope with a
  line stream on one channel and violates the clean-stdout guarantee.
- **Emit stdout by splitting the final buffer.** It is not real-time and
  defeats the purpose; the RFC explicitly forbids it.
- **Put JSONL/SSE/HTTP in the CLI.** Adds transport and product concerns to
  the composition root; View's SSE belongs to the View repository.
- **Publish all locals as a default debugging dump.** Unbounded
  confidentiality loss and no stable value encoding; rejected in favour of
  exact, bounded selections.
