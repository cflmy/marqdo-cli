# Marqdo AST → Program Mapping

Status: **current language contract, breaking rewrite (RFC-LANG-0002)**. This
file replaces the earlier Strong/Emph/List/Quote mapping in place; there is no
second active mapping and no per-file syntax selector.

> **Implementation state:** implemented by `marqdo-frontend` and
> `marqdo-expression`; the workspace gate is green at `ffd1cc9` (see
> `docs/STATUS.md` and `docs/evidence/2026-11-21-semantics-v2-green.md`).

Authority: `docs/architecture/RFC-LANG-0002-unified-semantics.md`.
Markdown parsing remains `marqdo.md.v1` (`spec/markdown-profile.md`); only the
Markdown→program mapping changes.

## 0. The mapping in one page

1. **Headings declare the semantic mode.** `@exec` and `@knowledge` may prefix
   any H1–H6 heading. Unmarked headings inherit the nearest ancestor heading's
   mode. The document root is Knowledge.
2. **Only an Exec H2 is a callable function.** H1 is a namespace/top-level
   unit, H2 Exec is a function, H2 Knowledge is an inert knowledge unit, and
   H3–H6 are sections that never create callable functions.
3. **Closed carriers carry one statement each.** In an Exec scope, `Strong`,
   `Emph` and inline `Code` are interchangeable carriers of one payload. They
   no longer mean execute/return/lazy respectively.
4. **Keywords inside the payload decide the operation.** `if`, `else if`, `else`, `while`, `break`,
   `continue`, `end`, `return`, `params(...)`, assignment and expression statements. Lists,
   block quotes and paragraphs are document containers only.
5. **Errors never fall back to prose.** Once a payload is extracted from an
   Exec scope it must parse fully; a failure is a diagnostic.
6. **Control blocks never cross a heading segment.** Every `if`/`while` must be
   closed by `end` inside the same heading's direct body. A `return` unwinds
   the whole H2 function.

Two rules override everything else:

* **Node type and structural position decide whether a node is a payload
  candidate**, never the success or failure of parsing and never natural
  language.
* **Invalid payloads are hard errors**, never "ordinary Markdown".

## 1. Document regions and heading modes

### 1.1 Declaring a mode

A heading mode declaration is recognized only when it is the exact prefix of
the heading's **first plain Text child**, followed by whitespace and a non-empty
heading name:

```ebnf
heading_mode_prefix = "@exec" | "@knowledge" ;
heading_decl        = heading_mode_prefix , whitespace , nonempty_title ;
```

The two legal words are case-sensitive and complete. The rules:

| Source | Result |
|---|---|
| `## @exec main` | Exec heading, semantic name `main` |
| `## @knowledge 原理` | Knowledge heading, semantic name `原理` |
| `## main` | no declaration; inherit |
| `## 关于 @exec 的讨论` | no declaration; the `@exec` is ordinary title text |
| `## @Exec main` / `## @knowledg main` | `sem.invalid_mode_marker` (no case/spelling repair) |
| `## @exec` | `sem.empty_unit_name` |
| `## @exec @knowledge main` | conflicting declarations; hard error |
| `## **@exec** main` | not a declaration; `@exec` is not the first plain Text child |
| prose containing `@exec` | ordinary text; never changes mode |

The heading's **display text** is unchanged for Markdown rendering. Its
**semantic name** has the declaration (and the whitespace after it) removed,
then applies the existing NFC/trim/whitespace-collapse normalization. All
identity, duplicate checks, function registration and `ref` lookup use the
semantic name; `@exec`/`@knowledge` are never part of a callable name.

ATX headings are the only accepted declaration form. A Setext heading has no
prefix declaration in the first version and simply inherits its mode; do not
invent an implicit declaration for it.

### 1.2 Inheritance

Using the Markdown heading stack (nearest lower-level ancestor heading, by
source order):

```text
Mode(document root) = Knowledge
Mode(H) = Explicit(H)             if H has a valid declaration
Mode(H) = Mode(nearest ancestor)  if H has no declaration and has an ancestor
Mode(H) = Knowledge               if H has no declaration and no ancestor
```

Inheritance is a structural calculation over heading levels, not inference from
prose. A heading's scope ends at the next heading of the same or higher level
(same or smaller number); deeper headings are inside it. Siblings never share a
mode: an unmarked heading after a sibling that explicitly switched to Knowledge
still inherits from the nearest **ancestor**, not from the sibling. Mechanically,
the ancestor stack is first reduced by removing the heading itself and every heading
of the same or higher level; only then is `Mode(nearest ancestor)` read. A
post-`3.0.0-beta.1` bugfix (RFC-EXT-0003 H-06) corrected an implementation
that read the inherited mode before reducing the stack, which could leak a sibling's
mode into an unmarked heading.

Skipped levels (H1 directly followed by H3) still use the nearest valid
ancestor heading.

### 1.3 Heading roles

| Heading | Mode | Language object | Callable? | Effect |
|---|---|---|---|---|
| H1 | Knowledge/Exec | Namespace or top-level unit | No | Sets the default mode for descendants |
| H2 | Exec | Function + Unit | **Yes** | Opens the function body |
| H2 | Knowledge | Knowledge unit | No | Document section; no function |
| H3–H6 | Exec | Section of the owning H2 Exec function | No | Appends an execution segment to that function |
| H3–H6 | Knowledge | Section (knowledge or under a function) | No | Retained as document; no statements |

H3–H6 never create functions, never export callables and never create a new
call frame. An Exec payload under an H1 without an H2 Exec function, or under a
Knowledge H2, is `sem.exec_without_function`; it is never promoted to a function
and never executed as module-level code.

### 1.4 Version selection

There is no syntax-version switch. Every `.mq.md` file, imported module,
dependency, fixture and stdlib module uses this mapping. Introducing
`marqdo_semantics`, `syntax: v1`, `--legacy`, `--syntax` or any per-file mode
selection is an error (`sem.unsupported_version_directive` where the old
directive is seen); no implicit downgrade to an earlier mapper is permitted.

## 2. Inert regions

The following are never searched for payloads, even inside an Exec scope:

| Region | Treatment |
|---|---|
| every `@knowledge` subtree | zero executable statements |
| heading text (including a mode declaration) | unit identity only |
| link labels and URLs, image alt text | document/link data |
| GFM table cells | table data |
| math / math blocks | inert `Formula` resources |
| HTML blocks/inline HTML | document data |
| fenced and indented code blocks | inert `Code` resources |
| front matter | module configuration |
| strikethrough and other non-MVP extensions | inert document data |

Inline `Code` (a code span) is a payload carrier in Exec scope. A fenced code
block is a different AST node and stays inert; the two must not be conflated.

## 3. Executable payload carriers

### 3.1 Accepted carriers

Only these Comrak AST nodes are payload carriers in the first version:

| AST kind | Payload |
|---|---|
| `Strong` | concatenation of its **direct** Text and inline Code children, in source order |
| `Emph` | same as `Strong` |
| inline `Code` | the code span literal |

They are **interchangeable carriers of one statement**. For the same Exec
scope, owner function and payload text:

```text
Parse(Strong("return n")) == Parse(Emph("return n")) == Parse(Code("return n"))
```

The original AST kind is still recorded for rendering and source spans. Format
does not select the operation.

### 3.2 Extraction rules

1. Traverse an Exec heading's direct body in Markdown AST document order, and
   recurse only through the allowed containers `Paragraph`, `List`, `ListItem`
   and `BlockQuote`.
2. Extract only complete outer carrier nodes. A nested `Code` inside a
   `Strong`/`Emph` is consumed by the outer carrier and must not execute again.
3. For `Strong`/`Emph`, only direct Text and inline Code children are allowed.
   Any nested `Strong`, `Emph`, `Link`, `Image`, math, line break or other
   complex child is `sem.payload_nested`. Never extract a partial statement or
   silently concatenate multiple fragments.
4. A payload must be fully consumed by exactly one statement. `x = 1; return x`
   in one carrier is `sem.syntax`; write two carriers instead.
5. Empty carriers in Exec scope are `sem.empty_payload`; they are never ignored.
6. Every payload keeps its original UTF-8 file span. For `Strong(Code(...))`
   record the segment mapping so diagnostics point into the inner text, not at
   the emphasis delimiters.
7. Two adjacent carriers produce two statements in source order. One carrier is
   never split into multiple statements by newlines or semicolons.

### 3.3 Containers

Containers provide order and layout, not operation type:

| Container | Compiler behavior | Not allowed |
|---|---|---|
| `Paragraph` | extract permitted carriers in source order | prose does not execute |
| ordered `List` | recurse through items in source order | no implicit `Branch` |
| bullet `List` | recurse through items in source order | no implicit `Loop` |
| `BlockQuote` | recurse through nested blocks in source order | no implicit `Call` |
| `Heading` | switches/extends mode and segments | not a statement |

A list's `ordered`/`start`/`tight` attributes, an item's checkbox, and quote
nesting depth never affect control flow. A plain ordered or bullet list with no
carriers has an empty execution projection (zero statements), which is not the
same as producing a `List([])` value.

### 3.4 The deliberate emphasis cost

In an Exec scope, `**重要**` is a payload and is parsed as the expression
`重要`. If no such symbol exists, that is `symbol.unknown`. The language does
not guess that the author wanted emphasis. Put purely visual emphasis in a
`@knowledge` section (or another inert region). This is a deliberate
determinism trade-off, not an implementation gap.

## 4. Statements

Each extracted payload is exactly one statement, defined by
`spec/expression.ebnf`:

```text
if <expr>       else if <expr>   else       end
while <expr>    break            continue
return <expr>   params(...)      assignment expression
```

Lexical details:

* Keywords are recognized only as complete first tokens: `ifdef(...)` and
  `returning` are ordinary identifiers; a bare `if`/`else` used as an
  identifier is a reserved-keyword error.
* `if`, `else if`, `while` and `return` require whitespace followed by an
  expression that consumes the whole payload. Trailing colons are
  `sem.syntax`. `else` and `end` take no arguments.
* `break` and `continue` are loop control only when the complete payload is
  the single identifier. `break 2` and `continue label` are syntax errors.
  `break(1)`, `continue = 1` and `return break` retain their ordinary call,
  assignment and expression meanings. To read a variable named `break` or
  `continue` as an expression statement, parenthesize the identifier.
* `elif`, `for`, `try` and semicolon-separated statements are outside this
  grammar.
* `return` always requires an expression; falling off the end of a function
  still returns `Unit`. Explicit `return None` is available when `None` is the
  intended value.
* `params(...)` is a compile-time signature declaration, not a runtime
  callable; see §6.
* Existing expression precedence, literals, calls, indexing, assignment and
  `Memory` place writes are unchanged. Conditions must evaluate to `Bool` and
  keep the existing purity rule.

## 5. Control-stack lowering

Within one execution segment, payloads form an ordered statement stream.
The innermost open control block owns its body.

* `if` opens a branch with zero or more subsequent `else if` arms and one
  optional final `else`. Each arm starts in its own carrier. Two carriers
  containing `else` and `if` still open a nested branch and require separate
  closing `end` statements. An empty arm is legal.
* `else if` and `else` must bind the innermost open block, which must be an
  `if`. A second `else` or an `else if` after `else` is a hard error.
* `end` closes the innermost open `if` or `while`. Reaching a segment
  boundary with an open block is a hard error on the opener.
* All conditional arms lower to one ordered `Branch`. Evaluate conditions
  left to right and execute only the first matching arm. If all are false,
  execute the optional `else` body or proceed to the next statement.
  Each conditional arm has its own source span.
* `while` lowers to `Loop`. `break` exits the nearest open loop and
  `continue` skips the rest of its body and rechecks its condition. Both
  require a loop in the same function and heading execution segment.
  `return` exits the whole function, including from inside a loop.
* Definite-assignment inference merges reachable fallthrough paths. Loop
  exits include condition-failure and `break` paths. `continue` and
  ordinary body fallthrough lead back to the condition. An abrupt transfer
  stops inference from its remaining sequence.

## 6. Functions, signatures and calls

* Only an Exec H2 creates a callable function. Its body is the concatenation,
  in source order, of its direct Exec body segment and the Exec segments of its
  H3–H6 descendants that are part of the same function. Knowledge segments
  contribute nothing.
* H3–H6 Exec sections share the H2 function's frame; locals assigned in one
  segment are visible in later segments. They do not create new functions.
* `params(name: Type, ...)` is legal only as the **first direct execution
  payload of an Exec H2 function**, at most once, and not inside a list or
  block quote. Recognition happens in the same Exec-mode traversal that finds
  statements, so a `params(...)` code span in a Knowledge section is inert
  documentation and is never a signature. The position is function-wide: it may be preceded by prose or a
  Knowledge subsection, but not by an executing statement in **any** heading
  segment of that function (a later `### @exec` section may not add a
  signature after earlier code ran). Violations are `sem.signature_position`;
  a duplicate declaration anywhere in the function is also rejected.
* A function without `params` keeps the existing free-variable inference.
  As before, an inferred signature accepts **named arguments only**; declare
  `params(...)` to accept positional arguments. Public functions should declare
  `params`; the optional `sem.inferred_signature` lint is emitted for an
  inferred signature that actually has parameters.
* Calls are ordinary expressions. `print(1)`, `f(n=5)` and `> **print(1)**`
  all call because the payload contains a call; the block quote itself does
  not create a call. A plain `> print(1)` is inert prose.
* `@knowledge` content is never scanned for identifiers, free variables or
  calls.
* `main` must be an Exec H2 to be the default `run` entry; if none exists, `run`
  reports `sem.no_entry_function`.

## 7. Segments and the heading boundary

An **execution segment** is one heading's direct body, excluding descendant
headings. Segments are ordered by source position. For each Exec H2 function,
process its own segment followed by the Exec segments of its descendant
headings, in document order.

The control stack is reset and required to be empty at every heading boundary.
A control block opened in one heading's direct body must be closed in that same
body; `end` in a later H3/H4 or on the other side of a Knowledge heading is
`sem.unmatched_end` there, while the opener's segment reports
`sem.unclosed_control`. The two errors are reported independently and are never
"repaired" across the boundary.

Complete segments are appended to the enclosing function body in source order;
earlier segments' assignments remain visible to later ones.

## 8. Diagnostics

Stable machine-readable codes (the human `message`/`hint` text may change):

| Code | Trigger |
|---|---|
| `sem.invalid_mode_marker` | unknown/case-variant/conflicting `@...` declaration at heading start |
| `sem.empty_unit_name` | a declaration with no non-empty heading name |
| `sem.duplicate_unit_name` | duplicate canonical heading/unit name in one scope |
| `sem.exec_without_function` | an executable payload with no owning H2 Exec function |
| `sem.payload_nested` | a `Strong`/`Emph` with a disallowed nested node |
| `sem.empty_payload` | an empty carrier in an Exec scope |
| `sem.syntax` | malformed statement, missing condition/value, trailing tokens, colon, unsupported keyword |
| `sem.unmatched_else` | `else` with no open `if` at the top of the stack |
| `sem.unmatched_else_if` | `else if` with no open `if` at the top of the stack |
| `sem.else_if_after_else` | `else if` after the final `else` of the same `if` |
| `sem.break_outside_loop` | `break` with no open loop in this heading segment |
| `sem.continue_outside_loop` | `continue` with no open loop in this heading segment |
| `sem.duplicate_else` | a second `else` for the same `if` |
| `sem.unmatched_end` | `end` with no open control frame |
| `sem.unclosed_control` | an open `if`/`while` at a heading/function/file boundary |
| `sem.signature_position` | `params` not the first function-wide direct payload, duplicated, or in a container |
| `sem.unsupported_version_directive` | legacy `marqdo_semantics`/syntax-selection front matter |
| `sem.no_entry_function` | `run` with no Exec H2 `main` |
| `sem.inferred_signature` | lint: public function without explicit `params` |

Existing `expr.*`, `symbol.unknown`, `runtime.*`, `resource.*` and capability
diagnostics keep their meaning. Diagnostics are ordered by
`(source_file, start_byte, code)`, and each carries an exact UTF-8 span plus a
related span when an opener or conflicting declaration is involved. A file with
any hard error is never executed.

## 9. Security and the runtime boundary

`@exec` is a compile-time projection, not a capability grant. Resource, file,
network, stdin/stdout, memory and budget checks are unchanged. A payload in
`@knowledge` produces zero IR, so it can never cause a host effect, even if its
text looks like `resource.open(...)`. Permission flags never change the
AST→IR result; they are checked at runtime by the existing host boundary.

The mapping introduces no `HostOp` for `if`/`else`/`while`/`end`/`return`, no
second interpreter and no runtime knowledge of Markdown emphasis. It lowers to
the existing `Assign`/`Eval`/`Branch`/`Loop`/`Return` IR.

## 10. Migration table (old mapping → current mapping)

| Requirement | Old source | Current source |
|---|---|---|
| Namespace | `# 算法` | `# @knowledge 算法`, or `# @exec 算法` for an Exec default |
| Function | `## main` | `## @exec main` (or an unmarked H2 under an Exec H1) |
| Assignment | `**x = 1**` | `**x = 1**`, `*x = 1*` or `` `x = 1` `` |
| Return | `*x*` | `**return x**`, `*return x*` or `` `return x` `` |
| Branch | ordered list with a first code-span condition | `if cond ... else ... end`; containers are presentation only |
| Loop | bullet list with a first code-span condition | `while cond ... end` |
| Call | block quote containing a call, or a Strong expression | any carrier whose expression is a call |
| Signature | first standalone code span `(n: Int)` after the H2 | `params(n: Int)` as the first function-wide direct payload |
| Ordinary list | error inside a function | ordinary document structure, zero statements |
| Ordinary quote | error inside a function | ordinary document structure, zero statements |
| Knowledge | implied by position/inertness only | explicit `@knowledge` scope |

Old files are not compatible. Migration must be done by AST role, not by a
global regular expression, and a migrated program must preserve function names,
argument meaning, evaluation order, early-return behavior and side effects.

## 11. Canonical example

```markdown
# @knowledge 数学演示

这是普通文档；**加粗**只显示为加粗。

## @exec main

`x = 5`
`y = x * x`
`return y`
```

Normalized IR: `Assign(x, 5)`, `Assign(y, Mul(x, x))`, `Return(y)`. The H1 is
Knowledge; the H2 overrides to Exec and is the callable `main`.

## 12. Non-negotiable implementation constraints

Do not:

* try to parse a carrier and fall back to prose on failure;
* infer a list is a branch because an item looks like a condition;
* use emphasis type to choose execute vs return;
* use block-quote/list type to choose call/branch/loop;
* scan raw Markdown text for `@exec`/`if`/`return` instead of using the AST;
* recover a misspelled `@knowledge` by inheriting the ancestor mode;
* expose a per-file syntax switch, legacy mapper, `--legacy` flag or dual
  active specification.
