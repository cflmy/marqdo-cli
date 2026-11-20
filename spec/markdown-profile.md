# Marqdo Markdown Profile v1

Status: **frozen for M0.5**. Changes require an ADR, updated goldens and a
profile id bump.

Profile id: `marqdo.md.v1`
Parser: [comrak](https://comrak.ee/) `=1.0.0`, `default-features = false`
CommonMark baseline: 0.31.2

## 1. Why a profile

Markdown is the syntax of the language. Any parser behaviour that can change
the AST can change program meaning, so the parser version and every extension
flag are part of the language version, not an implementation detail. Marqdo
never maintains a second Markdown parser and never decides semantics from raw
text with regular expressions.

## 2. Frozen parser options

Enabled extensions:

| Option | Value | Consequence |
|---|---|---|
| `extension.table` | `true` | GFM tables produce `table`/`table_row`/`table_cell` nodes. |
| `extension.strikethrough` | `true` | `~~x~~` produces `strikethrough`. Inert unless it appears in an executable payload (where it is an error). |
| `extension.tasklist` | `true` | `- [x]`/`- [ ]` items carry a checkbox flag on the item node. |
| `extension.math_dollars` | `true` | `$...$` and inline `$$...$$` produce `math`; standalone `$$` fences with `math_dollars_block` produce `math_block`. |
| `extension.math_dollars_block` | `true` | Multi-line `$$` display math is a block node. |
| `extension.front_matter_delimiter` | `Some("---")` | YAML front matter is recognised only at the start of the document. |

Disabled (must stay `false`): `autolink`, `footnotes`, `inline_footnotes`,
`description_lists`, `multiline_block_quotes`, `alerts`, `ofm_callouts`,
`superscript`, `subscript`, `underline`, `highlight`, `insert`, `spoiler`,
`subtext`, `shortcodes`, `wikilinks_*`, `grid_table`, `greentext`,
`ofm_*`, `block_directive`, `header_attributes`, `fenced_code_attributes`,
`inline_code_attributes`, `link_attributes`, `cjk_friendly_emphasis`,
`phoenix_heex`, `tasklist_in_table`, `relaxed_tasklist_matching`,
`relaxed_autolinks`; parse options `smart`, `ignore_setext`,
`escaped_char_spans` stay `false`.

The unit test `profile::tests::profile_options_are_frozen` asserts this table.

## 3. Byte spans and source maps

Every AST node carries a half-open byte span `[start, end)` into the original
UTF-8 file. Spans are produced from the parser's line/column positions (byte
columns) with a line index, so multi-byte characters are handled correctly.

* `# 数学` has span covering exactly `# 数学` (8 bytes).
* A UTF-8 BOM is tolerated at offset 0. The BOM is stripped before parsing and
  all spans are re-based onto the original file (the first character after the
  BOM starts at byte 3 in that case).
* Spans of container nodes cover their full source region; inline spans cover
  the inline source, not the surrounding markers.
* The source map is used for diagnostics and IDE navigation only. No semantics
  is derived from the source text.

## 4. Front matter

YAML front matter is recognised only when the parser reports a `front_matter`
node, i.e. the document starts with `---` (after an optional BOM) and a closing
`---` or `...` line exists. A `---` later in the document is a thematic break
or a Setext heading underline, never front matter.

The parser node keeps the raw delimiters; Marqdo extracts the YAML body and
parses it exactly once with `serde_yaml`. Supported semantics:

* `import:` mapping from local alias to module address (see
  `references.md`). Imports are explicit; nothing is inferred.
* Any other key is metadata for tools and is ignored by the interpreter.
* Invalid YAML is a diagnostic (`frontmatter.invalid`), never silently dropped.
* Front matter never executes.

## 5. AST view

`marqdo inspect --ast --json` prints this tree:

```json
{
  "kind": "document",
  "span": {"start": 0, "end": 20},
  "children": [
    {"kind": "heading", "level": 2, "setext": false, "span": {"start": 0, "end": 7},
     "children": [{"kind": "text", "span": {"start": 3, "end": 7}, "literal": "阶乘"}]}
  ]
}
```

Rules:

* Node kinds mirror the profile; `task` list items are `item` nodes with
  `"task": true|false` so that node type still determines list structure.
  GFM task items replace the parser's item node; the profile normalises them
  back to `item` plus the checkbox flag.
* Literal payloads are kept on inert leaves: `text`, `code`, `code_block`,
  `math`, `math_block`, `html_block`, `html_inline`, `front_matter`, and the
  `raw` payload of unmodelled nodes.
* `link`/`image` embed `url` and `title`; `heading` embeds `level` and
  `setext`; `list` embeds `ordered`, optional `start` and `tight`; `table`
  embeds column `alignments`; `table_row` embeds `header`; inline `code`
  embeds `ticks` (delimiter length) so payload offsets can be mapped back to
  exact source bytes (a code span's source span includes its backticks).
* Unmodelled parser nodes are preserved as `{"kind":"other","tag":"..."}`
  with their children and raw literal. They are inert for the language.
* Attribute order in JSON is fixed by the serializer and goldens are
  regenerated only with an explicit update command.

## 6. Structural equivalence

Program semantics depend on node type and position, not on source spelling:

* `**x**` and `__x__` are both `strong` and behave identically.
* `*x*` and `_x_` are both `emph`.
* `## name` and a Setext level-2 heading are both `heading` with `level: 2`.
  (`setext` is recorded for inspection only.)
* Ordered list delimiters `1.` / `1)` and bullet markers `-`/`*`/`+` produce
  the same `list`/`item` structure; role does not depend on the marker.
* Tight versus loose lists (`tight`) only affects rendering; it does not
  change program semantics.

## 7. Contract with readers

A `.mq.md` file is rendered normally by any CommonMark/GFM renderer (math
needs a math extension). Rendering never triggers execution. Code fences and
math remain inert `Code`/`Formula` values until an explicit call.
