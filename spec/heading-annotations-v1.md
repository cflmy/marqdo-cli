# Heading Annotations v1 — PROPOSED, NOT IMPLEMENTED

Status: **Proposed. The current build does not implement heading annotations.**
The parser still recognises only the closed Core Modes `@exec` and `@knowledge`;
any other leading `@name` is a hard `sem.invalid_mode_marker` and there is no
`annotation.unknown` code in the current binary. This page is a future contract
accepted by [ADR-0060](../docs/adr/0060-heading-modes-and-extensible-annotations.md)
and targeted at `3.1.x` (Step B). Nothing here may be advertised as available
until Step B ships with its own evidence.

Audience: Markdown/frontend implementers, Analysis v3/sidecar implementers, and
future View/Test/Prove tools. Current language semantics are in
[`mapping.md`](mapping.md) and cannot be changed by this proposal.

## 1. Two layers

```text
ATX heading first plain-Text prefix
├── Core Mode            closed: @exec | @knowledge        (decides execution)
├── Heading Annotations   open:   @namespace.identifier …   (metadata only)
└── semantic_title        non-empty plain heading text     (FunctionId/anchors)
```

An annotation **never** decides whether a section executes. The Core Mode does.
Unmarked headings inherit the nearest ancestor mode; annotations do not participate
in that calculation.

## 2. Grammar (v1)

```ebnf
heading_prefix   = [ core_mode, whitespace ], { annotation, whitespace }, semantic_title ;
core_mode        = "@exec" | "@knowledge" ;
annotation       = "@", namespace, ".", identifier ;
namespace        = lower_identifier, { ".", lower_identifier } ;
identifier       = lower_identifier, { "-" | "_" | digit | letter } ;
semantic_title   = nonempty_plain_heading_text ;
lower_identifier = lower_letter, { lower_letter | digit } ;
```

Example (future syntax, not accepted by the current CLI):

```markdown
# @knowledge 算法手册

## @exec @org.example.test 最大公约数

`params(a: Int, b: Int)`

`return a`

## @knowledge @org.example.fold 实现说明

说明正文。
```

`semantic_name` is the normalized title after removing the accepted prefix
tokens; it alone feeds FunctionId and anchors. Byte spans of the source are
unchanged and existing `@exec` documents keep the same semantic name.

## 3. Constraints

1. No annotation parameters in v1. No Python decorator expression, parentheses
   evaluation or JSON inside a heading. Parameters, if ever needed, use a separate
   declared table/metadata revision.
2. At most one Core Mode, and it must come before any annotation. `@exec` +
   `@knowledge` together is a hard core-mode conflict. There is no third mode.
3. Annotation names are namespaced (`@org.example.test`); an unqualified global
   `@test` that any tool may claim is forbidden.
4. Annotations never participate in mode inheritance. v1 attaches to the heading
   only.
5. `display_title` (render text) and `semantic_name` (identity) are distinct.
6. The parser extracts tokens by fixed grammar and **never executes annotation
   provider code**; a static manifest validates whether an annotation is allowed.
7. Undeclared annotations produce a deterministic static error
   (`annotation.unknown`); body prose may still discuss `@foo`; a heading-title
   escape must be specified and tested (for example an explicit escape token).
8. Unknown Core Mode spellings (`@Exec`) remain errors and are never re-read as a
   third-party annotation.
9. An annotation may not add callability, change the AST/IR, reorder execution,
   reduce budgets, grant capability, load native code or change a bound call.
   Runtime ignores annotations completely.
10. Repositories using only `@exec`/`@knowledge` keep byte-identical parse
    behaviour. The H-06 sibling-mode bugfix is a separate language bugfix and is
    recorded in `spec/mapping.md` §1.2.

## 4. Registration manifest (proposal)

```json
{
  "schema": "marqdo.heading-annotations",
  "schema_version": 1,
  "owner": "org.example",
  "annotations": [
    {
      "name": "org.example.test",
      "allowed_modes": ["exec"],
      "heading_levels": [2],
      "repeatable": false,
      "metadata_only": true
    },
    {
      "name": "org.example.fold",
      "allowed_modes": ["exec", "knowledge"],
      "heading_levels": [2, 3, 4, 5, 6],
      "repeatable": false,
      "metadata_only": true
    }
  ]
}
```

- The manifest is a non-executable resource of an explicitly declared,
  version-pinned project/dependency package; `marqdo.yaml` references it and its
  identity is covered by the lock/project snapshot digest.
- Duplicate ownership/name, unknown schema version and tampered content fail
  statically; the manifest never runs code.
- Do **not** silently add a fourth `ExtensionKind` to lock v4. Prefer the manifest
  as a locked non-executable resource; a lock top-level annotation record requires
  a lock protocol version bump and migration tests.

## 5. Analysis projection (future, versioned)

Analysis v2 `units[]` keeps `id/kind/name/level/span/function` unchanged. Future
metadata uses Analysis v3 or a `heading-annotations.v1` sidecar, for example:

```json
{
  "schema": "marqdo.heading-annotations-sidecar",
  "schema_version": 1,
  "source_digest": "sha256:<64 hex>",
  "analysis_digest": "sha256:<64 hex>",
  "units": [
    {
      "unit_id": "root:pkg@0.1.0::doc.mq.md#ns/unit",
      "function_id": "root:pkg@0.1.0::doc.mq.md::ns.f",
      "effective_mode": "exec",
      "mode_origin": "inherited",
      "annotations": [
        {"name": "org.example.test", "owner": "org.example", "manifest_digest": "sha256:<64 hex>"}
      ],
      "span": {"start": 10, "end": 20}
    }
  ]
}
```

The sidecar is bound to both the source digest and the Analysis bytes digest;
a consumer must reject a stale pairing. No private IR is required.

## 6. Security prohibitions

- An annotation cannot grant a file/network/process capability; grants stay in
  the existing capability flow.
- An annotation cannot flip `@knowledge` to `@exec`, add a callable, load native
  code or change a CallBinding.
- A consumer without the tool (View/Test/Prove) still runs `check`/`run`
  deterministically; results do not depend on installed tools.
- `pure`/`strict` continue to reject execution extensions; annotations are static
  data only.

## 7. Acceptance matrix (FUTURE, enable only after Step B)

| ID | Input/action | Expected |
|---|---|---|
| T-ANNO-01 | `## @exec @org.example.test f` | Function name is still `f`; static annotation is readable |
| T-ANNO-02 | Valid annotation on a Knowledge heading | Not callable, not executed |
| T-ANNO-03 | Undeclared annotation | Static `annotation.unknown` |
| T-ANNO-04 | Forged/digest-mismatched manifest | Fails at `check`; third-party code never runs |
| T-ANNO-05 | Duplicate/conflicting/level-illegal annotation | Static error with stable code/span |
| T-ANNO-06 | Annotation present, Test Tool absent | Execution result identical |
| T-ANNO-07 | Install/uninstall tool, same source/lock/CLI | Runtime IR and function results unchanged |
| T-ANNO-08 | `@exec @knowledge` together | Fixed core-mode conflict |
| T-ANNO-09 | Annotation tries to grant file permission | Impossible; still denied without a grant |
| T-ANNO-10 | Real Unicode Chinese title | Source spans match Markdown AST bytes |
| T-ANNO-11 | Sidecar paired with another source SHA | Consumer rejects stale metadata |

Until Step B is executed and evidenced, docs/STATUS must label this
**Proposed/not implemented** and no test row above is a PASS.
