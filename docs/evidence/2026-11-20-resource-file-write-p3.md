# RFC-CM CM-A06 extension evidence: buffered project-file writes

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0–P3 changes.
Decision record: [ADR-0026](../adr/0026-buffered-project-file-write-streams.md)
Related: [ADR-0025](2026-11-20-resource-stream-p3.md),
[STATUS §5](../STATUS.md).
Scope: project-file **write** extension only; no network/process/directory.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  406 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Implemented behavior

`resource.open("file", "<project-relative path>", ["write"])`:

* project mode only; requires an `fs_write` glob grant and the same
  `ProjectFs` path policy (reject absolute, `..`, symlink escapes);
* pure mode refuses file resources by mode; strict refuses every resource open;
* the writer owns an in-memory buffer; `resource.write` appends and returns
  `{ok:true, value:{count}}` without touching the filesystem;
* `resource.close` flushes through `ProjectFs::write_bytes` and then marks the
  handle closed; a failed flush leaves the handle open with its buffer;
* an unclosed writer never creates the file (no flush on drop);
* buffer ceiling is the existing 8 MiB file limit; an oversized append is
  `resource.exhausted` and leaves both buffer and file unchanged;
* `resource.read` on a writer, and `resource.write` on a reader, are
  `resource.unsupported`;
* no new `Value`, `HostOp`, Markdown node or wire shape.

## Tests

### Host integration (`crates/marqdo-host/tests/resource_streams.rs`, 9)

```text
pure_mode_writes_stdout_through_a_resource
stdout_write_obeys_the_output_budget_atomically
strict_mode_refuses_every_resource_open
resource_handles_cannot_cross_the_generic_host_boundary
pure_mode_refuses_file_resources_even_with_a_grant
project_file_writes_buffer_until_close_and_require_a_grant
file_write_buffer_has_a_hard_ceiling
project_file_reads_are_chunked_and_require_a_grant
traversal_and_unsupported_rights_are_refused
```

The write test asserts the file does not exist before `close`, appears with
`abcdef` after `close`, closed read/close return `resource.closed`, and the
same open without `fs_write` is `permission.denied`.

### CLI integration (`crates/cli/tests/resource_streams.rs`, 4)

```text
project_file_stream_writes_on_close_then_reads_back
  run index.mq.md --allow-write out.bin --allow-read out.bin --json
  write count 3, close ok, read count 3 eof true, bytes 65/66/67
  out.bin on disk = "ABC"

file_copy_is_written_in_mqmd_through_chunked_streams
  .mq.md opens data.txt for read and out.bin for write,
  loops resource.read(16) / resource.write(count) until eof,
  closes both; total 6; out.bin on disk = "abcdef"
  (C09 file copy without a Rust copy_file builtin)

project_file_stream_reads_chunks_into_memory
file_stream_requires_a_read_grant
```

### Host table unit tests (`crates/marqdo-host/src/resource.rs`, 3)

Chunked EOF, forged/closed handles, stdout-not-readable and file-not-writable
(read-only direction) remain covered; the writer uses the same handle checks.

## What this does not prove

* No append/truncate/seek, directories, metadata, network, process, clock,
  random, device or generic provider registry.
* No crash-atomic replace: `ProjectFs::write_bytes` is one validated write, not
  a temp-file rename.
* No file write in `pure` or `strict`.
* No CM-A07 stdlib `print` migration.
* No Windows/macOS evidence and no remote CI URL.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-host --test resource_streams
cargo test -p marqdo-cli --test resource_streams
cargo test --workspace --locked
```
