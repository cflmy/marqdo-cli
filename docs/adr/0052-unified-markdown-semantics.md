# ADR-0052: Unified Markdown semantics — heading modes, unified payloads and keyword control flow

Status: **Accepted (target contract; frontend implementation pending)**
Date: 2026-11-21
Supersedes: ADR-0002 in part (executable roles), the AST→program mapping in
`spec/mapping.md` and the statement layer in `spec/expression.ebnf`
Specs: `spec/mapping.md`, `spec/expression.ebnf`
Authority: `docs/architecture/RFC-LANG-0002-unified-semantics.md`
Tracks: `docs/migration/semantic-rewrite-inventory.tsv`

## Context

The previous language mapping binds program meaning to Markdown presentation:

* `Strong` executes;
* `Emph` returns;
* an ordered list is a branch;
* a bullet list is a loop;
* a block quote is a call;
* an H2 is unconditionally a function and a standalone code span after it is a
  signature.

That makes ordinary document structure impossible to use freely inside a
function, forces control flow to depend on list type and nesting, and gives
inline code a signature privilege no other carrier has. It also means a parse
failure can only be reported as a shape error, because there is no separate
statement syntax to parse.

## Decision

Adopt a single, breaking Markdown→program mapping. There is no V1/V2
coexistence, no per-file syntax selector and no `--legacy` path.

1. **Heading modes.** H1–H6 may start their first plain Text child with an
   exact `@exec` or `@knowledge` declaration followed by whitespace and a
   non-empty name. Unmarked headings inherit the nearest ancestor heading's
   mode; the document root is Knowledge. The declaration is stripped from the
   semantic heading name.
2. **Function boundary unchanged.** Only an Exec H2 is a callable function. H2
   Knowledge is an inert knowledge unit. H3–H6 are sections and never create
   callables. A payload with no owning Exec H2 is `sem.exec_without_function`.
3. **Unified payloads.** In an Exec scope, `Strong`, `Emph` and inline `Code`
   are interchangeable carriers of one statement. Only direct Text/Code
   children are extracted; nested carriers are `sem.payload_nested`; each AST
   carrier is extracted at most once.
4. **Keyword control flow.** A payload is exactly one of `if`, `else`, `while`,
   `end`, `return`, `params(...)`, assignment or expression. `if`/`while` are
   closed by an explicit `end`; `else` is optional. Lists, block quotes and
   paragraphs are containers with no instruction type.
5. **Segments.** A control block may not cross a heading boundary. Each
   heading's direct body is one segment; the stack must be empty at every
   boundary. Complete segments append to the owning H2 function in source
   order.
6. **Signatures.** An optional `params(...)` statement is legal only as the
   first direct payload of an Exec H2; otherwise free-variable inference
   applies. The old standalone code-span signature is removed.
7. **IR unchanged in kind.** Control words lower to the existing
   `Assign`/`Eval`/`Branch`/`Loop`/`Return`. No `HostOp`, no second
   interpreter, no runtime knowledge of Markdown.
8. **Security unchanged.** `@exec` is not a capability grant. `@knowledge`
   produces no IR and therefore no effects.
9. **Errors never fall back to prose.** A malformed payload in an Exec scope is
   a hard diagnostic; spelling errors in mode declarations are hard errors, not
   inherited modes.

## Consequences

* Every `.mq.md` source in the repository — stdlib, examples, fixtures, Rust
  embedded samples, docs — must be migrated. Old syntax must not remain a
  supported active language.
* `spec/mapping.md` and `spec/expression.ebnf` are replaced in place; there is
  no second active spec. Historical material is archived with a banner.
* Normal emphasis in an Exec section becomes a payload and may fail as an
  unknown symbol. Authors must use `@knowledge` sections for purely visual
  emphasis. This is accepted as the price of determinism.
* Until the Rust frontend, expression parser, bundled stdlib and fixtures are
  migrated, the implemented CLI does **not** accept this syntax. The tree must
  not be released or described as complete during that window. See
  `docs/STATUS.md`.
* Test fixtures are migrated as text; execution goldens are expected to be red
  until the implementation lands. No test may silently keep asserting the old
  mapping as current.
* Migration is AST-role based, not a global regex rewrite; ambiguous cases stop
  for manual rewrite.

## Alternatives rejected

* **Keep both mappings per file.** Rejected: two active specs, unstable
  semantics for the same source, permanent maintenance and accidental
  execution.
* **Infer control flow from list shape.** Rejected: ordinary prose lists would
  execute and parse errors could silently become documents.
* **Use `mq exec` fenced blocks only.** Rejected: loses native Markdown
  document structure and the language's purpose.
* **Bind `end` to indentation or list nesting.** Rejected: reformatting would
  change control flow.
* **Treat emphasis type as intent.** Rejected: no deterministic way to tell
  emphasis from execution from the AST.
