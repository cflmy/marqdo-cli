# Marqdo AST → Program Mapping v1

Status: **frozen for M0.5**. This file is the normative mapping from Markdown
AST structure to program meaning. It is derived directly from
`docs/DEVELOPMENT_BLUEPRINT.md` sections 1.2–1.4 and 3.2.

## 1. The two rules that override everything

1. **Node type and structural position decide the role.** A `strong` node in an
   executable position is an execution statement and its payload must parse as
   a `Statement`. It is never "prose that happens to look like code".
2. **Invalid executable shapes are hard errors.** If a node occupies an
   executable role but its shape is invalid, compilation fails with a
   `markdown.invalid_construct` diagnostic. There is no fallback to prose.

Prose never causes an action. Natural-language words (“if”, “return”, “若”,
“执行”) are never interpreted as keywords.

## 2. Document regions and heading scopes

A document root is a module. Heading nodes create addressable scopes:

| AST | Role | Requirements |
|---|---|---|
| `document` | module | exactly one root |
| `heading(level=1)` | namespace / object domain | non-empty canonical name |
| `heading(level=2)` | function | non-empty canonical name, unique in its scope |
| `heading(level>=3)` | section (source location) | does not create a function |

Scope rules:

* The range of a heading extends until the next heading of the same or a
  lower level (i.e. same or smaller number), or the end of the document.
* A level-2 heading without a preceding level-1 heading belongs to the module
  root. Nested headings inside a section remain part of the enclosing function.
* Headings inside container blocks (block quotes, list items) or inside fenced
  code blocks never create functions.
* A function's canonical name is the normalised text of its heading children:
  NFC, trimmed, internal whitespace collapsed to one space. Empty names are
  errors. Duplicate canonical names in the same scope are errors. GitHub-style
  anchor slugs are derived for navigation but are **not** the identity.
* `main` at module root is the default entry point for `marqdo run`; an
  imported module's `main` is not run automatically.

## 3. Lazy (inert) regions

The following nodes are inert regardless of where they appear. Their inline
content is never executed:

* heading text (headings scope, they do not run)
* `link` labels and `image` alt text
* `table` cells (tables are document values)
* `strikethrough` wrappers: their content is inert prose and is not traversed
  for execution; do not rely on formatting to hide executable nodes
* `code_block` and inline `code` outside designated payload positions
* `math` / `math_block`
* `html_block` / `html_inline`
* front matter
* text and other inline nodes inside prose paragraphs

Inline formatting inside those regions (e.g. `strong` inside a heading) does
not execute. The implementation must use a context-aware visitor, not a blind
whole-tree walk.

## 4. Executable roles in function bodies

Inside a function (`heading(level=2)` range) the following roles apply,
including inside branch and loop bodies:

| AST node | Role | Shape requirement | On invalid shape |
|---|---|---|---|
| `strong` | execution statement | strict payload parses as `Statement` | hard error |
| `emph` | return statement | strict payload parses as `Expression` | hard error |
| `list(ordered=true)` | branch | each item starts with a condition header | hard error |
| `list(ordered=false)` | loop | each item starts with a loop condition header | hard error |
| `block_quote` | call | one paragraph forming a complete call | hard error |
| `paragraph` | prose + inline execution | inline executable nodes run in order | n/a |
| everything else | inert document value | — | type/constraint errors only if used |

Outside functions (module top-level prose) the same executable nodes are inert
document content (S-03 in the blueprint: executable lists/strong/emph only run
inside a function body). The single exception is `main` itself, which is a
function. To make this explicit rather than silent, the frontend emits a
`markdown.inert_executable` **warning** for every would-be execution node found
outside a function body; the document still compiles and the node does nothing.

Headings inside container blocks (block quotes, list items) or fenced code never
create functions and never execute inline markup.

### 4.1 Strict payload extraction

For `strong`/`emph`, the payload is the concatenation of the direct `text` and
`code` child literals, exactly as written. Nested `strong`, `emph`, `link`,
`strikethrough`, line breaks, math, HTML or any other inline node is an error:
one AST node must never be executed twice or hide control content.

`**x = 1**` and `**`x = 1`**` have the same payload `x = 1`. `**a * b**`
parses as text in most cases; when Markdown emphasis and the multiplication
operator conflict, the author should use a code span for the payload:
`` **`a * b`** ``. There is no text-based second parse.

### 4.2 Paragraph and document order

Executable inline nodes in a paragraph run in source order. Blocks run in AST
document order. A `return` short-circuits the remaining nodes of the enclosing
function, including nodes inside branches and loops.

### 4.3 Branch (`ordered list`)

Each item must:

1. contain at least one block;
2. have a `paragraph` as its **first** block;
3. have an inline `code` node as the **first inline node** of that paragraph;
   its literal is the condition expression;
4. any remaining inline content in that first paragraph is prose.

The item's remaining blocks form the branch body. Items are tested in AST
order; the first condition evaluating to `true` is executed once. No match
means `Unit` and execution continues after the list. Conditions must be
`Bool`; they must be pure (side effects in conditions are a static error when
detectable and a runtime capability concern otherwise).

### 4.4 Loop (`bullet list`)

Each item is an independent `while` loop. Item structure matches the branch
header rule: the first paragraph's first inline node must be a `code` span
(condition), and the remaining blocks are the loop body. The condition is
re-evaluated before every iteration and must be `Bool`. Items execute in
order, so a bullet list is a sequence of loops, one per item.

Iteration over collections is expressed with stdlib `map`/`filter`/`fold`;
a dedicated `forEach` header is intentionally not part of v1.

### 4.5 Call (`block quote`)

A block quote is exactly one `paragraph` and nothing else. The paragraph is
parsed with the call grammar:

* plain payload: `打印("你好")`, `math.eval(ref("#公式"), {"r": r}, target="A")`
  — the strict payload must parse as a complete `CallExpr`;
* link form: `[计算面积](./math.mq.md#计算面积)(r=3)` — the first inline node
  must be a `link`, its destination identifies the target function, and the
  remaining `text`/`code` payload must be a parenthesised argument list.

Anything else (a second paragraph, plain prose, a nested quote, an expression
that is not a call) is a hard error. Two calls require two block quote nodes.
`> [说明](#目标)` without an argument list is not accepted in v1.

The call result is discarded by the statement form; use `strong` to bind it
(`**面积 = 计算面积(r=3)**`).

## 5. Calls, returns and values

* A call is `函数(...)`; zero-argument calls still write `()`.
* Positional arguments are only allowed for functions with an explicit
  signature. Named arguments can be given in any order for any function.
* Arguments are evaluated in source order exactly once before binding.
* `emph` returns the value of its expression from the enclosing function
  (propagating through branch/loop bodies), never from a block.
* `*表达式*` used in a branch is a return from the whole function.
* A function that ends without a return evaluates to `Unit`. `None` is an
  explicit empty value and is distinct from `Unit`.
* Return at module top level (outside any function) is an error.

## 6. Function signatures and parameter inference

* Optional explicit signature: the first `paragraph` immediately after the
  function heading, if and only if it consists solely of one inline `code`
  span, and that span parses as `(name: Type = default, ...)`.
  The declaration is inert Markdown otherwise.
* Without a signature, parameters are inferred from **free reads of executable
  expressions** using data-flow analysis over the function's control-flow
  graph (see `evaluation.md` §5). Natural language, heading words, code spans
  outside executable positions and prose never introduce parameters.
* Inferred parameters are identified by name. Functions with inferred
  signatures accept **named arguments only**. Explicit signatures allow
  positional arguments in declaration order.
* A read that is neither a parameter, a local assignment, a module constant,
  an imported binding nor a builtin is a `symbol.unknown` error with a hint;
  the CLI can list inferred signatures with `inspect --signature`.

## 7. Example: ordered list branch

Source:

```markdown
## 分类

1. `n < 0`
   **结果 = -1**
2. `n == 0`
   **结果 = 0**
3. `n > 0`
   **结果 = 1**

*结果*
```

AST (abridged):

```text
heading(2) 分类
  list(ordered) start=1
    item
      paragraph -> code("n < 0"), text("")
      strong -> text("结果 = -1")
    item
      paragraph -> code("n == 0")
      strong -> text("结果 = 0")
    ...
  paragraph
    emph -> text("结果")
```

IR:

```text
Function 分类(params=[n])
  Branch [
    (Eval(n < 0), [Assign(结果, -1)]),
    (Eval(n == 0), [Assign(结果, 0)]),
    (Eval(n > 0), [Assign(结果, 1)]),
  ]
  Return(Ident(结果))
```

## 8. Hard-error checklist

The following are compile errors with source spans, never prose fallbacks:

* `strong`/`emph` in a function body whose payload does not parse;
* nested `strong`/`emph`/link inside an executable payload;
* ordered list item whose first inline node is not a code span condition;
* bullet list item without a condition header;
* non-`Bool` condition (detected statically when the type is known, else at
  runtime as `runtime.type`);
* block quote that is not exactly one complete call;
* duplicate function names in a scope;
* empty heading names;
* return outside a function;
* executable node inside a lazy region being interpreted (must not happen).
