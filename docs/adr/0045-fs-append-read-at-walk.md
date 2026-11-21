# ADR-0045: `fs.append`, `fs.read_at` and `fs.walk`

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-fs-extended.md`

## Context

ADR-0041 added metadata/directory operations, but file content still required
a whole-file `resource.open` read and a whole-buffer write. That leaves append,
offset reads and recursive directory discovery unavailable to `.mq.md` without
adding Rust algorithms to the standard library.

## Decision

Extend the existing `fs.*` group of `resource.request`:

```text
resource.request("fs.append",  [path, text])       -> {ok:true, value:count}
resource.request("fs.read_at", [path, offset, len])-> {ok:true, value:Text}
resource.request("fs.walk",    [dir])              -> {ok:true, value:List<Text>}
```

* `fs.append` writes UTF-8 text to the end of a project file, creating it when
  missing. It requires `fs_write`, is bounded by the existing 8 MiB file limit,
  and never bypasses the path policy.
* `fs.read_at` reads at most `len` bytes from `offset` and returns Text. Both
  values are non-negative integers; `len` is capped at 8 MiB. Invalid UTF-8 is
  a fatal type error, not silent loss.
* `fs.walk` recursively lists regular files under a directory, relative to the
  project root. It requires `fs_read`, skips symlinks, has a depth limit of 64
  and a 4096-entry limit. The list is sorted by name at each level.
* All three are project-mode only, use the same path/grant checks as the other
  `fs.*` operations, and record one `resource.request` effect event.
* No new builtin or host trait method is added; the operation strings extend
  the existing ABI.

## Consequences

* `std:fs/file` exposes `追加文本`, `读取片段` and `遍历` in `.mq.md`.
* Binary offset reads, append handles and filesystem watch remain open; a
  watch API needs a separate event model and ADR.
* The operations allocate at most the existing 8 MiB read limit; `walk` is
  metadata-only and returns names, not contents.
