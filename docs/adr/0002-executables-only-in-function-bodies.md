# ADR-0002: Executable nodes execute only inside level-2 function bodies

> **Historical / superseded in part.** The execution-scope decision (payloads
> only inside an owning H2 function, never at module top level) remains, but
> the carrier/role rules in this ADR (`Strong` executes, `Emph` returns, lists
> and quotes are instructions) were replaced by
> [ADR-0052](0052-unified-markdown-semantics.md) and
> [`spec/mapping.md`](../../spec/mapping.md). The syntax examples below are the
> syntax at the time of the decision, not current syntax.

Status: **Accepted (historical mapping superseded by ADR-0052)**
Date: 2026-11-20

## Context

The blueprint (§1.2 execution context, §3.2 control flow, unresolved issue
**S-03**) establishes that `Strong` (execute), `Emph` (return), ordered lists
(branch), bullet lists (loop) and block quotes (call) have program meaning
**inside a function body**, i.e. inside the range of a level-2 heading. It
leaves the module top level under-specified: if a document has `**x = 1**`
before any function, should that be a hard error, a silent no-op, or an
explicit top-level statement?

Two failure modes had to be avoided:

1. **Silent execution at module scope.** There is no function frame, no return
   target and no entry point, so top-level effects would be surprising and
   would make `check`/`inspect` harder to reason about.
2. **Hard error.** Ordinary Markdown documents routinely use `**bold**`,
   `*italic*`, `1. ordered` and `> quotes` in prose. Treating every such node
   outside a function as a fatal compile error would make the language hostile
   to normal documentation and would break the "renders normally" goal.

The chosen baseline was: executable nodes only execute inside function bodies;
elsewhere they are ordinary document content, but this must not happen
silently.

## Decision

1. A **function body** is the child-block range of a `heading(level=2)` up to
   the next heading of level ≤ 2 (`crates/marqdo-frontend/src/units.rs`,
   `FunctionScope.body_start..body_end`). Executable roles are assigned only
   inside those ranges by `crates/marqdo-frontend/src/map.rs`.
2. Outside every function body, `strong`, `emph`, `list` and `block_quote`
   nodes are **inert document content**. They are not mapped to IR, are not
   executed, and prose is not inspected for keywords.
3. The frontend emits a **warning** `markdown.inert_executable` for every
   would-be execution node found outside a function body. The document still
   compiles and runs; only `check`'s diagnostic output reflects the warning.
   The warning is produced once per top-level node and has a hint to move the
   construct under a `## function` heading.
   * `block_quote` → "a block quote invocation ... is inert document content";
   * `list` → "a list control structure ...";
   * `paragraph` → the first `strong`/`emph` found in the paragraph
     ("an execution statement" / "a return statement").
   * warnings are emitted only while scanning the document's direct top-level
     blocks: headings, tables, code/math blocks and other non-paragraph blocks
     produce none. A top-level paragraph is walked recursively, so a
     `strong`/`emph` nested under a link or strikethrough inside that paragraph
     can still produce the inert warning. Nodes inside a function body produce
     no warning (they are mapped or rejected by the strict executable-shape
     rules instead).
4. Warnings do not change exit codes: `check` exits 0 when there are no error
   diagnostics even if warnings exist; `run`/`call` execute normally. JSON
   `check` output sets `success = (errors == 0)` and includes the warning
   entries in `diagnostics`.

Rationale for the nodes covered: these are exactly the block-level constructs
that `map.rs` would otherwise turn into statements if a function body existed.
Inline nodes inside a paragraph are covered by one paragraph-level warning
rather than one per node, which keeps normal prose readable.

## Alternatives

1. **Hard error for every executable-looking node outside a function.** Rejected
   (see context); it forbids emphasis in ordinary Markdown and makes writing a
   README a compile failure.
2. **Silent inert behaviour with no diagnostic.** Rejected: an author could
   believe top-level `**x = 1**` had executed. The blueprint explicitly wants
   invalid execution intent surfaced.
3. **Execute top-level nodes in an implicit module frame.** Rejected: no
   defined entry semantics, no return target, would make imported modules
   potential side-effect roots (violating "modules register definitions only").
4. **Compile-time error only when the payload parses as a statement.** Rejected:
   this is the forbidden heuristic ("parses as code → execute") from blueprint
   §1.3; role must come from node type and position, not from guessing.
5. **Warning per AST node.** Rejected as noisy for nested content; one warning
   per top-level construct is enough to point at the problem.

## Markdown AST shape

No new node kinds. Relevant structure:

```text
document
  heading(level=2) "f"        <-- opens a function body
    paragraph -> strong(text "x = 1")   <-- executes (Stmt::Assign)
  paragraph (before any heading)
    strong(text "x = 1")      <-- inert; emits markdown.inert_executable
  list(ordered)               <-- inert outside function; warning
  block_quote                 <-- inert outside function; warning
```

A level-2 heading without a preceding level-1 heading is a module-root function
(`## main`) and its body executes normally. A `###` section inside the function
range stays inside the function body; headings never execute.

## Compatibility

* Warning code `markdown.inert_executable` is part of the CLI diagnostic
  contract. Renaming it requires an ADR and a CLI version note.
* Making this a hard error later would be a breaking language change: documents
  that currently compile (with warnings) would stop. If that is ever desired it
  needs a migration plan (e.g. an opt-in strict lint).
* No package/ABI/IR shape change: inert nodes are simply absent from the IR.
* Prose invariance is preserved: editing narrative text outside functions
  cannot change the IR.

## Security

No effect: inert nodes never reach the interpreter, host or kernels. The
warning reduces the chance of a false belief that a side effect happened,
which is a security-relevant honesty property. A document cannot cause an
execute-by-accident through top-level prose.

## Testing

* `crates/marqdo-frontend/src/units.rs::tests::
  executable_outside_function_warns_but_does_not_execute` asserts the warning
  and that the function index is still built.
* `units.rs` tests for function ranges, nested sections, root-level `## main`
  and duplicate/empty headings.
* `crates/marqdo-frontend/src/analyze.rs` tests hard errors for invalid
  executable shapes inside function bodies (the companion rule: when a node
  *is* executable, its shape is strictly checked).
* Conformance/negative fixtures under `crates/cli/tests` and `tests/`
  (work in progress by the fixtures workstream) are expected to cover the
  warning and the inert behavior; `spec/mapping.md` §4 documents the contract.

## Migration & rollout

M0.5 ships with warnings. Rollout guidance:

* document authors move executable constructs under `## function` headings;
* tooling should show the warning in editors (the diagnostic already has a
  precise span);
* if a future release makes inert execution a hard error, add a deprecation
  window and a `check` flag so existing documents can be migrated.
