# ADR-0005: AST normalization (Setext headings, task items, code-span ticks, strikethrough)

> **Historical mapping note.** The profile/AST normalization rules in this ADR
> remain current. Its role claims — that ordered/bullet list structure
> "determines branch/loop roles" and that `strong`/`emph` carry execute/return
> roles — were superseded by
> [RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md) /
> [ADR-0052](0052-unified-markdown-semantics.md) and
> [`spec/mapping.md`](../../spec/mapping.md): lists and block quotes are
> presentation containers, and `Strong`, `Emph` and inline `Code` are
> interchangeable one-statement payload carriers. Strikethrough and the other
> inert regions stay inert.

Status: **Accepted**
Date: 2026-11-20

## Context

The language derives program meaning from the **canonical AST**, never from
source spelling (`spec/markdown-profile.md`, `spec/mapping.md` §1). The parser
(`comrak =1.0.0`, frozen options) normalizes several syntaxes before Marqdo
sees them, and a few normalizations are lossy:

* `# H` and Setext `H\n===` both become a heading node; the source spelling is
  lost unless recorded;
* GFM task list items (`- [x] text`) produce a task-specific item node;
* inline code spans are stored as a literal without the backtick delimiters,
  but source spans include them;
* `~~strikethrough~~` is enabled by the profile and wraps arbitrary inlines.

The language needs a single, documented answer per question, because node type
and structural position decide executable roles.

## Decision

The profile (`marqdo.md.v1`) and the canonical AST
(`crates/marqdo-markdown/src/ast.rs`, `src/profile.rs`) freeze these rules:

1. **Setext headings are headings.** `# name` and a Setext level-1/2 underline
   both produce `heading { level, setext }`; `setext` is inspection-only and
   **never affects semantics**. `## name` and a Setext level-2 heading are both
   functions. The profile keeps `parse.ignore_setext = false`.
2. **Task-list items are `item` nodes with a task flag.**
   `item { task: Some(true|false) }` for GFM task items, `task: None`
   otherwise. Node type stays `item`, so list structure (ordered vs bullet)
   still determines branch/loop roles; the checkbox flag is inert data and does
   not change execution. A task list item in an executable position still
   requires its condition header like any other item.
3. **Code spans carry delimiter tick counts.** Inline `code { ticks }` records
   the number of backticks, and the source span includes them; the literal is
   the inner payload. This lets the frontend map expression payload spans back
   to exact source bytes for diagnostics. "Ticks" is not user-visible program
   meaning.
4. **Strikethrough is inert/lazy.** `strikethrough` is an inert wrapper: it is
   never traversed for execution, its content is prose, and a `strong`/`emph`
   inside it does not execute. It is not an error to write it; the language
   simply does not use formatting as a control-flow mechanism here. (A code
   span or math inside it is likewise inert.)
5. **Unmodelled nodes are preserved** as `other { tag }` with children and raw
   literal. They are inert but never silently dropped from `inspect --ast`.
6. **Literal payloads are kept** on inert leaves: `text`, `code`, `code_block`,
   `math`, `math_block`, `html_block`, `html_inline`, `front_matter`.
7. **Other frozen normalizations:**
   * `**x**`/`__x__` and `*x*`/`_x_` are the same node kinds (`strong`/`emph`);
   * ordered delimiters `1.`/`1)` and bullets `-`/`*`/`+` are the same
     `list`/`item` structure; role depends on `ordered`, not marker;
   * tight vs loose lists only affect rendering (`tight` is recorded, inert);
   * `link`/`image` keep `url`/`title`; `table` keeps column alignments,
     `table_row` keeps `header`; `heading` keeps `level`/`setext`;
   * a UTF-8 BOM at offset 0 is stripped and all spans are re-based onto the
     original file;
   * front matter is recognized only at the document start (after BOM), with
     `---` opening and `---`/`...` closing; a later `---` is a thematic break
     or Setext underline.

These rules are part of profile id `marqdo.md.v1`. Changing them (or bumping
the comrak version/options) requires a profile id bump, updated goldens and this
ADR being superseded.

## Alternatives

1. **Distinguish Setext vs ATX headings semantically.** Rejected: identical AST
   structure outside the marker must have identical meaning (`## H` and Setext
   H2 are the same function). Keeping `setext` only for inspection avoids
   silently diverging programs when a document changes style.
2. **Introduce a separate `task_item` node kind.** Rejected: it would require
   every list-role check to special-case two item kinds and could be used to
   hide a control structure behind a checkbox. Normalizing to `item { task }`
   keeps list structure authoritative.
3. **Strip the tick information and store only the literal.** Rejected: payload
   spans returned by the expression parser are relative to the concatenated
   payload; without delimiter lengths, error spans cannot be mapped back to
   file bytes precisely.
4. **Traverse strikethrough content for execution.** Rejected: it is a lazy
   region by profile; executing inside it would make formatting semantics
   surprising ("struck-through code runs"). It would also create a second way
   to hide executable nodes.
5. **Drop unknown parser nodes.** Rejected: `inspect --ast` must show what the
   parser produced; dropping syntax hides profile drift.
6. **Preserve raw source and re-parse with a custom parser.** Rejected by the
   project's core rule: one standard Markdown parser, no second Markdown AST.

## Markdown AST shape

Frozen JSON shapes (`inspect --ast --json`):

```json
{"kind":"heading","level":2,"setext":false,"span":{"start":0,"end":7},
 "children":[{"kind":"text","span":{"start":3,"end":7},"literal":"阶乘"}]}
```

```json
{"kind":"list","ordered":false,"tight":true,"span":{...},
 "children":[{"kind":"item","task":true,"span":{...},"children":[...]}]}
```

```json
{"kind":"code","ticks":2,"span":{"start":10,"end":16},"literal":"a * b"}
```

```json
{"kind":"strikethrough","span":{...},"children":[{"kind":"text",...}]}
```

Attribute order in the JSON is fixed by the serializer; goldens are regenerated
only through the explicit update workflow.

### 5.1 Known interaction to keep in mind

Inside a function body, a `**strong**` (or an ordered/bullet list) nested under
`strikethrough` is inert and never executes; because the mapper does not
traverse strikethrough, it also produces no warning. Outside function bodies,
the `markdown.inert_executable` scan walks a top-level paragraph recursively,
so such a nested node may still produce the inert warning even though it would
not execute if the paragraph were inside a function. This is consistent with
"formatting never hides execution", but an author who intends execution must
not wrap it in strikethrough. Warnings for lazy-region content inside functions
are a possible future refinement; no behavior change is planned without an ADR.

## Compatibility

* Profile id `marqdo.md.v1` is frozen with comrak `=1.0.0` and the exact
  extension flags; the unit test
  `crates/marqdo-markdown/src/profile.rs::tests::profile_options_are_frozen`
  asserts the option table (tables, tasklist, strikethrough, math dollars,
  front matter; all other extensions disabled).
* Adding an AST field is a JSON output change for `inspect --ast`; it needs a
  goldens update and a profile note. Changing a semantic rule (e.g. executing
  inside strikethrough) is a language change and requires an ADR.
* No package/ABI/wire impact.

## Security

* The lazy-region rules ensure inline formatting inside headings, tables,
  links, code, math, HTML and front matter never executes — a context-aware
  visitor is used (`crates/marqdo-frontend/src/map.rs` only descends from
  paragraphs into `strong`/`emph`, and ignores every other inline node).
* Strikethrough cannot be used to smuggle executable content; it is inert.
* Normalizing task items to `item` prevents a checkbox from hiding a
  branch/loop header; the condition-header check still applies.
* No new capabilities or host calls.

## Testing

* `crates/marqdo-markdown/src/profile.rs`:
  * `profile_options_are_frozen` — exact extension/parse option table;
  * `setext_headings_are_heading_nodes` — Setext produces `heading { setext: true }`;
  * `tables_and_task_lists_are_supported` — task items carry `task: Some(_)`;
  * span/BOM tests.
* Frontend mapping tests use code spans as conditions and payloads, exercising
  `ticks`/payload span mapping.
* `spec/markdown-profile.md` §5/§6 is the normative JSON shape; AST goldens
  under `crates/cli/tests` (fixtures workstream) regenerate only with the
  documented update command.
* Conformance fixtures are expected to include `**a**` vs `__a__`, Setext vs
  ATX, task/list variants and strikethrough inertness.

## Migration & rollout

No migration for documents: Setext and ATX were always the same function under
this profile. When comrak is upgraded or the extension set changes, bump the
profile id, regenerate AST goldens, re-run all specs/ADRs that reference node
shapes, and document the change here (superseding this ADR if a rule changes).
Code-span `ticks` and `task` are additive inspection fields; keep them optional
in consumers that parse the JSON.
