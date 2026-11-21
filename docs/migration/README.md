# Semantic rewrite: migration guide and inventory

Status: **migration in progress**. The target language contract is
[RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md) and
[`spec/mapping.md`](../../spec/mapping.md). The Rust frontend and bundled
stdlib are **not yet migrated**, so executable tests are expected to be red
until the implementation lands. See [`../STATUS.md`](../STATUS.md).

This directory records the repository-wide migration required by the RFC:
every active `.mq.md` source, example, fixture and documentation snippet must
use the single new syntax. Old syntax is not an active language.

## Current state (2026-11-21)

- **Text migration: complete.** Specs, active docs, examples, all non-stdlib
  test/fixture `.mq.md` files, the security-lane embedded fixtures and
  Rust-embedded `.mq.md` literals were rewritten. 1009 present tracked+new files are
  classified in the inventory; there are no `pending` text items.
- **Implementation: pending.** 51 inventory rows remain
  `pending-implementation`: the 9 frontend/expression/IR files that implement
  the mapping and the 42 stdlib fileset entries (sources + manifest).
- **Verification:** `cargo fmt --check`, `cargo check --workspace --tests`,
  `conformance_ast` (4/4) and `docs_consistency` (4/4) pass. The semantic,
  negative, property and CLI suites that execute Marqdo source are expected red
  against the pre-RFC frontend. Evidence:
  [`../evidence/2026-11-21-semantic-docs-migration.md`](../evidence/2026-11-21-semantic-docs-migration.md).
- Old v1 compile-negative fixtures were deleted and replaced by new-semantic
  negatives; they are intentionally absent from the inventory rather than
  counted as reviewed.

## Migration rules for this repository

1. **One syntax.** Do not add `marqdo_semantics`, `--legacy`, `syntax: v1` or a
   second example. Old syntax is either migrated or marked historical.
2. **Exec H2 is explicit.** Write `## @exec 名称` for every callable function,
   even under an Exec H1. Heading names keep the original semantic name
   (`## @exec 阶乘`, not `## @exec @exec 阶乘`).
3. **Mode on H1.** Prefer an explicit `# @knowledge 名称` for document
   namespaces and `# @exec 名称` only when the whole namespace is executable.
4. **One payload, one statement.** Use a standalone inline code span by default
   (`` `x = 1` ``) because it is Markdown-unambiguous. Use `**...**` or
   `*...*` when the fixture is specifically testing carrier equivalence.
5. **Control flow.** `if <expr>` … `[else]` … `end`; `while <expr>` … `end`.
   A control block never crosses a heading. `return <expr>` replaces the old
   italic return.
6. **Branch migration.** An ordered list with N arms is *first true arm wins*.
   Convert N arms to a nested if/else chain (an inner `if` inside `else`), not
   to N independent `if`s. A single arm becomes `if`. A bullet list is a
   sequence of independent `while` loops, one per item, in item order.
7. **Calls.** `> print(x)` was an old call; in the new language it is inert
   prose. Write `> **print(x)**`, `` `print(x)` `` or a plain paragraph
   payload. A block quote itself never calls.
8. **Signatures.** The first standalone code span `(n: Int)` becomes
   `params(n: Int)` as the first direct payload of the Exec H2. Keep parameter
   names, order and defaults.
9. **Knowledge.** Purely visual emphasis and explanatory lists belong in
   `@knowledge` sections. A `Strong` inside an Exec section is a payload and
   must be a valid statement.
10. **Preserve meaning.** Do not change function names, argument names/defaults,
    evaluation order, early-return behavior, values, stdout or expected error
    intent. For `.expected.json` value goldens (return value/args/stdout), keep
    the file unless the semantic name really changed.
11. **Expected diagnostics.** Old shape errors about lists/quotes/emphasis
    become valid inert Markdown, not errors. Replace them with new negatives for
    `sem.*` codes (see `spec/mapping.md` §8). Keep genuine runtime negatives
    (bounds, type, permission) and only migrate their source spelling.
12. **No silent golden regeneration.** Do not run `UPDATE_*_GOLDENS=1` to mask
    a semantic change. A golden update needs a stated new-semantic reason.
13. **Tests are text-only here.** Rust test strings and the `init` template
    text are migrated; the frontend, expression parser and stdlib `.mq.md`
    sources are not. Those implementation items are tracked as pending in the
    inventory. Do not change protocol schemas or security logic in this
    migration.
14. **Use the review aids, not them as proof.** `tools/dev-only/check-mq-syntax.py`
    catches unbalanced control blocks and unknown `sem.*` codes;
    `tools/dev-only/gen-migration-inventory.py` regenerates the inventory. Both
    are audit aids; the real gate is the migrated frontend plus the RFC §12
    acceptance matrix.

## Conversion examples

Old:

```markdown
## 分类

1. `n < 0`

   *-1*

2. `n == 0`

   *0*

3. `n > 0`

   *1*
```

New (first-true chain, explicit `end`):

```markdown
## @exec 分类

`if n < 0`

`return -1`

`else`

`if n == 0`

`return 0`

`else`

`return 1`

`end`

`end`
```

Old loop:

```markdown
## 求和

`(n: Int)`

**i = 0**
**sum = 0**

- `i < n`

  **sum = sum + i**
  **i = i + 1**

*sum*
```

New:

```markdown
## @exec 求和

`params(n: Int)`

`i = 0`
`sum = 0`

`while i < n`

`sum = sum + i`
`i = i + 1`

`end`

`return sum`
```

## Inventory

`semantic-rewrite-inventory.tsv` is generated from the tracked tree with
`python3 tools/dev-only/gen-migration-inventory.py` and records each file's
category, action and status. Columns:

```text
path	category	active	action	verification	status
```

Categories: `spec`, `architecture`, `tutorial`, `example`, `stdlib`, `test`,
`rust-test`, `historical`, `non-semantic`. Status: `migrated`, `reviewed`,
`historical`, `pending-implementation`, `pending`.

The final gate is **100% of tracked files classified** and all active
`.mq.md` files migrated, not a grep with no matches.
