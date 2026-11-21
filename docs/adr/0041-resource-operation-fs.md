# ADR-0041: Non-streaming Resource Operation for filesystem metadata and directories

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0025, ADR-0026, ADR-0040
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-21-fs-file-product.md`, `docs/evidence/2026-11-21-resource-operation.md`

## Context

`resource.open/read/write/close` is a byte-stream ABI. It cannot express
operations that do not return a byte stream: `exists`, `metadata`, directory
listing, `mkdir`, `remove` and `rename`. Adding one Rust builtin per operation,
or faking them by reading a directory as a file, would either grow the language
surface without a contract or overstate what a stream can do.

The audit RFC's P1 items require a separate, explicit capability class for
these non-streaming operations, not a silent `HostOp` for every filesystem
call.

## Decision

Add one builtin and one host trait method:

```text
resource.request(operation: Text, arguments: List) -> Result
Host::resource_request(operation: &str, arguments: Vec<Value>, ctx) -> Result<Value, RuntimeError>
```

* The first version registers only the `fs.*` operation group. An unknown
  operation is fatal `resource.unsupported`; it is not a recoverable Result.
* Arguments must be `Text` paths. `Memory` and `Resource` handles are rejected
  before dispatch, so they still cannot cross the host boundary.
* Every call is project-mode only. `pure` and `strict` return
  `permission.denied`; this does not widen strict L1.
* Grants: `fs.exists`/`fs.metadata`/`fs.list` require `fs_read`; `fs.mkdir`/
  `fs.remove` require `fs_write`; `fs.rename` requires `fs_write` on both
  paths. Grants are checked before path resolution or any I/O.
* Paths use the same `ProjectFs::resolve` policy: relative, no `..`, no
  absolute path, canonical target must stay inside the project root.
* Expected filesystem conditions are recoverable `{ok:false, code, message}`:
  `fs.not_found`, `fs.not_directory`, `fs.exists`, `fs.not_empty`. Capability
  denial, traversal, unsupported operations and I/O failures stay fatal
  `RuntimeError`s.
* `remove` deletes a file or an **empty** directory; it never recurses.
  `rename` refuses an existing target with `fs.exists` instead of overwriting.
* `mkdir` is single-level: a missing parent returns `fs.not_found`.
* Every call records one semantic `resource.request` and one effect event
  with `op=fs.<name>`. No bytes move through the operation; file content still
  uses `resource.read`/`resource.write`.

## Consequences

* The language surface grows by exactly one builtin (`resource.request`), not
  one per operation. The operation strings are a versioned stdlib ABI, not new
  Markdown syntax.
* A future directory walk, seek, stat or process operation can reuse the same
  shape without inventing a new `HostOp`; it still needs its own ADR and
  capability group.
* `std:fs/file` is a `.mq.md` facade over both ABIs: byte streams for content,
  `resource.request` for metadata/directories. No Rust algorithm is added.
