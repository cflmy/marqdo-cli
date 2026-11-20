# ADR-0026: Buffered project-file write streams (CM-A06 extension)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A06, CM-A09, CM-A11), ADR-0024, ADR-0025
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-resource-file-write-p3.md`

## Context

ADR-0025 froze the first resource ABI slice with `stdio/stdout` write and
project-file **read**. A standard library cannot implement file copy, file
format writers or "write then verify" without a write-capable file resource.
The RFC's P3 target is explicit `open/read/write/close` for project files in
`project` mode; strict must remain fail-closed.

Direct per-write file I/O would expose partial writes and OS error timing to
every caller and would make it easy to leave a half-written project file. This
ADR adds the smallest honest write path: an in-memory buffer per file writer,
flushed through the existing project path policy on `close`.

## Decision

### 1. Provider shape

Add a third stream shape to the host resource table (ADR-0025):

```text
resource.open("file", "<project-relative path>", ["write"]) -> Resource
```

* `project` mode only, with an `fs_write` glob grant matching the normalised
  relative path.
* `pure` still refuses file resources by mode check; `strict` still refuses
  every `resource.open` (CM-A11 fail-closed).
* Absolute paths, `..`, symlink escapes and non-regular targets are rejected by
  `ProjectFs` exactly like the existing `fs.write`.
* A writer owns an in-memory byte buffer. `resource.write` appends bytes and
  returns `{ok:true, value:{count}}`; it does **not** touch the filesystem.
* `resource.close` flushes the buffer through `ProjectFs::write_bytes` and then
  marks the handle closed. If the flush fails, the handle stays open with its
  buffer intact and the error is reported; the caller may retry or close again.
* If the program exits without closing, no file is written. This is a
  deliberate "no flush on drop" rule: a crash or a missing `close` must not
  silently publish a partial file.
* `resource.write` is rejected on a file reader; `resource.read` is rejected on
  a file writer; stdout remains write-only. Wrong-direction use is
  `resource.unsupported`.
* A writer buffer is capped at the existing 8 MiB project read/write ceiling
  (`MAX_READ_BYTES`). Exceeding it is `resource.exhausted` and the append is
  atomic: the buffer and file are unchanged.

### 2. Rights and modes

| Kind | Descriptor | Rights | Mode | Grant |
|---|---|---|---|---|
| `file` | project-relative path | `["read"]` | project | `fs_read` glob |
| `file` | project-relative path | `["write"]` | project | `fs_write` glob |
| `stdio` | `stdout` | `["write"]` | pure/project | `console` |

Mixed rights (`["read","write"]`) and other kinds/rights are
`resource.unsupported` in this slice. There is no truncate/append/seek flag:
every writer starts empty and replaces the file on flush.

### 3. Errors, budgets and tracing

* `resource.closed` for read/write/close on a closed handle (recoverable).
* `runtime.resource` for forged/cross-run/stale handles (fatal).
* `resource.exhausted` for a buffer over the byte ceiling.
* `permission.denied` for a missing mode/grant or a path policy failure.
* `resource.unsupported` for a direction/rights combination not implemented.
* `resource.write` on a file writer does not charge the program output budget;
  the buffer is bounded by the file ceiling. `resource.close` performs one
  `ProjectFs::write_bytes` call and records `resource.close` with the byte
  count, never the contents.
* No partial file is visible on a failed close because the write is a single
  `std::fs::write` to the validated project path (the same atomicity level as
  the existing `fs.write`; no temp-file rename is claimed).

### 4. Explicit non-goals

* No append/truncate/seek flags, no random access, no directory or metadata
  operations.
* No file write in pure or strict mode.
* No network/process/device/clock/random resources.
* No generic provider registry (CM-A09), no async/nonblocking I/O.
* No claim that `ProjectFs::write_bytes` is crash-atomic; it is a single
  validated write, not a transactional replace.
* No CM-A07 `print` migration in this ADR.

## Compatibility

* Additive to ADR-0025: existing `file` read and `stdio` write behavior is
  unchanged; `file` write was previously `resource.unsupported`.
* No new `Value` variant, `HostOp`, Markdown node or wire shape.
* Error categories/codes are unchanged; the new `resource.exhausted` diagnostic
  reuses the existing stable `resource.exhausted` code family.

## Consequences

* `.mq.md` can now implement file copy and bounded format writers by opening a
  read stream and a write stream, transferring bytes through `Memory`, and
  closing both. This is C09's core shape without a `copy_file` Rust builtin.
* Buffered close gives deterministic "all or nothing within one write call"
  behavior and keeps OS write errors at a point where the caller still has a
  Result path.
* Large files require chunked writes; the 8 MiB ceiling is the first-slice
  limit, not a permanent design.

## Verification

`docs/evidence/2026-11-20-resource-file-write-p3.md` records host unit tests
(grants, pure/strict refusal, traversal, buffer ceiling, retry-after-failure),
CLI tests for write-then-read and missing grants, and the full workspace gate.
