# ADR-0017: Runtime errors carry logical source locations

Status: **Accepted**
Date: 2026-11-20
Supersedes: the entry-file-only runtime span rule in ADR-0006

## Context

`Span` is a byte range in one source file. The interpreter produced spans in the
module where the failure occurred, but `RuntimeError` did not record which
module. The CLI therefore rendered every runtime error against the entry file
(`crates/cli/src/commands.rs`), so an error on line 5 of an imported module
could be reported at line 5 of `index.mq.md`, or with a meaningless column.

Span alone is not a source location. With package-aware `ModuleId`s (ADR-0008)
the logical location `(ModuleId, Span)` is available and stable across physical
paths and package instances.

## Decision

### 1. `RuntimeError` and `CallFrame` carry a logical module

`marqdo-value::RuntimeError` gains:

* `module_id: Option<String>` — canonical `ModuleId` containing `span`;
* `origin: ErrorOrigin` — `source` (Marqdo source) or `callsite` (host, plugin
  or foreign boundary where no callee-internal stack exists).

`CallFrame` gains `module_id: Option<String>` for the callsite in the caller.
`push_frame(function, span, module_id)` records both.

### 2. Attribution happens at module boundaries, not at every constructor

* Every function body is executed with a known `FunctionId.module`. Errors
  escaping `exec_stmts` get that module if they do not already have one, so
  innermost callee attribution is preserved.
* Argument-binding errors are attributed to the caller module at the call site.
* Errors crossing a host/plugin/foreign boundary are marked `origin: callsite`
  and attributed to the Marqdo callsite module. No foreign-internal stack is
  fabricated.
* Existing `RuntimeError::new(category, span, message)` call sites remain valid;
  attribution is additive.

### 3. CLI resolution has no entry-file fallback

`LoadedProject::locate(module_id, span)` parses the logical module, finds the
loaded file, rejects out-of-range spans, and derives line/column with that
file's `LineIndex`.

Runtime error JSON keeps `span: {start, end}` and adds `module_id`, `origin`,
`line`, `column`, `file`, `relative_path`. Each frame carries `function`,
`module_id`, `span`, and resolved line/column/file when available. If the
module is unknown or the span is invalid, the fields are absent/null and the
original module id and byte span are still emitted. Never clamp a span or fall
back to the entry file.

Human rendering prints the primary location from the located module. Frames
print their callsite file/line or the logical `module@start..end` when it cannot
be resolved.

Physical `file` fields are local convenience for humans; they are not part of
the versioned Analysis/Trace protocols and may differ across machines.

## Consequences

* Cross-module runtime diagnostics now point at the module that failed and
  keep caller frames resolvable, including two package instances that share a
  relative path.
* Span offsets remain UTF-8 byte offsets into that module's exact source.
* The JSON shape is additive for existing consumers; `span.line`/`span.file`
  move to top-level `line`/`file` and are also duplicated per frame.
* Compile-time diagnostics already carried a module; this change brings runtime
  errors to the same model.
* Any code constructing a synthetic `RuntimeError` without a module still
  renders with an explicit `<unknown>` location rather than a guess.

## Tests

* `runtime_error_in_imported_module_reports_that_module` (CLI, real binary):
  division by zero in an imported module reports that module's file and line,
  both in JSON and human output; the callsite frame points at the entry file.
* Existing runtime/conformance tests continue to pass with additive fields.
