# Marqdo Analysis

Current public contract for `marqdo check --emit PATH`. Every project emits
`marqdo.analysis` with `schema_version: 3`.

## 1. Invariants

`check --emit`:

* never executes the document, loads a plugin shared library, starts a foreign
  kernel or imports a module implicitly;
* validates the loaded program first; on a compile error it writes no artifact
  and removes an existing artifact at the target path;
* writes atomically through a temporary file and rename;
* contains no absolute machine paths, timestamps, PIDs or other environmental
  data, so the same logical project emits byte-identical Analysis from
  different checkout directories and is safe to compare across machines.

## 2. Top-level shape

```json
{
  "schema": "marqdo.analysis",
  "schema_version": 3,
  "producer": {
    "name": "marqdo",
    "version": "<crate version>",
    "markdown_profile": "marqdo.md.v1"
  },
  "project": {
    "root_id": "root:quantum@0.1.0",
    "identity_kind": "manifest",
    "lock_digest": "sha256:<64 hex>",
    "stdlib": {
      "version": "0.1.0",
      "digest": "sha256:<64 hex>",
      "instance_id": "std:marqdo-stdlib@0.1.0#<64 hex>"
    }
  },
  "modules": [],
  "diagnostics": []
}
```

`project.identity_kind` is:

| Kind | `root_id` | Meaning |
|---|---|---|
| `manifest` | `root:<project.name>@<project.version>` | Logical identity of a project with `marqdo.yaml`. Moving the checkout or a Git clone does not change it. |
| `workspace_snapshot` | `sha256:<64 hex>` | No manifest. Digest of sorted `(module_id, source_digest)` pairs; identifies content, not a persistent project. |

`project.lock_digest` is the SHA-256 of the exact `marqdo.lock` bytes prefixed
with `sha256:`, or `""` when there is no lock. It is not a logical identity.

`project.stdlib` records the verified bundled standard library identity used by
this load: declared version, content digest and package instance id. It is
`null` when no bundle was available. A digest mismatch is a hard load error, so
an Analysis artifact can only name a bundle whose bytes matched the manifest.

`AnalysisDigest` is the lowercase hex SHA-256 of the exact UTF-8 bytes written
by `check --emit` (pretty JSON). This is what Observation binds to
`source.analysis_digest`; an independent consumer can verify it by hashing the
file. Diagnostics and any configuration options that change the projection are
part of that digest.

Module ids are `<package instance>::<package-relative path>` (ADR-0008) and are
scoped by `project.root_id`. A module id alone is not a cross-project symbol.

## 3. Modules

The module object is unchanged from Analysis v1 except for `resources`:

```json
{
  "id": "root:res@0.1.0::index.mq.md",
  "package_instance_id": "root:res@0.1.0",
  "relative_path": "index.mq.md",
  "source_digest": "<64 hex of exact source bytes>",
  "units": [],
  "functions": [],
  "imports": [],
  "references": [],
  "calls": [],
  "resources": []
}
```

Modules are sorted by `id`; functions, units and calls keep document order and
the v1 semantic node shapes (`assign`, `expression`, `branch`, `loop`,
`return`; literals, variables, lists, records, unary/binary, member, index,
call). The v1 `call_site` node was retired with RFC-LANG-0002: calls are
`call` expressions inside an `assign`/`expression`/`return`/condition body, not
a separate statement. `imports`, `references` and `calls` remain static facts; a
Markdown reference is not a call. `effects` remains `["unknown"]` (a
conservative summary, not a purity proof).

`calls[]` is projected from the single prepared call binding
(`spec/callable-resolution.md` §6): `state` remains
`function`/`builtin`/`plugin`/`unresolved`, and official stdlib aliases such as
`打印` are reported as `function` targets because that is the implementation
that runs. The private `CallBindingTable` is not serialized; Analysis's
schema and version are unchanged.

An `assign` node keeps the v1 string `target` for a local rebinding. Addressable
memory adds one additive target form (ADR-0022); consumers that only understand
local assignments can ignore an object target:

```json
{"kind":"assign",
 "target":{"kind":"index",
           "base":{"kind":"variable","name":"内存","span":{"start":23,"end":26}},
           "index":{"kind":"literal","type":"int","value":"0","span":{"start":27,"end":28}},
           "span":{"start":23,"end":29}},
 "value":{"kind":"literal","type":"int","value":"7","span":{"start":32,"end":33}},
 "span":{"start":18,"end":35}}
```

`target.kind` is `"index"`; `base` and `index` use the same expression node
shapes as every other expression. Analysis is static: it records the place but
never allocates a region or executes the write.

## 4. Resources

`resources` is an array, not a count. The projection includes exactly the
resource kinds represented by `marqdo-ir::Resource`; it does not invent table,
HTML or link resources.

```json
{
  "id": "root:res@0.1.0::index.mq.md#resource:formula-1",
  "module_id": "root:res@0.1.0::index.mq.md",
  "unit_id": "root:res@0.1.0::index.mq.md#calc",
  "kind": "formula",
  "span": {"start": 30, "end": 44},
  "content_digest": "<64 hex>",
  "language": null,
  "display": true
}
```

| Field | Semantics |
|---|---|
| `id` | `<module_id>#resource:<kind>-<n>`; `n` is the 1-based ordinal among resources of that kind in the module, in document order |
| `module_id` | redundant with the enclosing module, present for flattened consumers |
| `unit_id` | innermost owning unit |
| `kind` | `formula` or `code`; future kinds are additive and unknown kinds must be ignored |
| `span` | Markdown node byte span, including math delimiters/fence, relative to that module's source |
| `content_digest` | SHA-256 of the payload: formula LaTeX bytes without delimiters; exact code fence source bytes |
| `language` | code fence info string (may be `""`); `null` for formulas |
| `display` | formula only: block vs inline math |

Resource ids are Analysis-local ordinals, not persistent symbols and not `ref`
anchors. Content identity is `(module_id, content_digest)`; location identity is
`(module_id, span)`. Inserting an earlier resource renumbers later ordinals, but
the content digest and span still let a consumer detect the change. Payloads
are not copied into Analysis: consumers read the module source (via
`relative_path`) and slice `span` or verify `content_digest`.

## 5. Diagnostics

Diagnostics no longer embed absolute physical paths:

```json
{
  "module": "root:res@0.1.0::index.mq.md",
  "relative_path": "index.mq.md",
  "span": {"start": 10, "end": 20},
  "line": 2,
  "column": 3,
  "severity": "warning",
  "code": "sem.inferred_signature",
  "message": "...",
  "hints": []
}
```

`span` is the primary coordinate; `line`/`column` are presentation-derived from
the owning module. `span`, `line` or `column` may be `null` when no precise
location exists.

The `code` value is an open, stable string: the current language revision adds
the `sem.*` family (`spec/mapping.md` §8) and removes the old
`markdown.inert_executable` warning. Diagnostic codes may be added without changing the structure.

## 6. Loop transfers

Conditional arms appear in source order. Both `break` and `continue` are
statements with a `kind` and the half-open UTF-8 `span` of their carrier.
They have no operands or call edges. Walk the enclosing bodies to find
the nearest loop. `break` transfers past that loop and `continue` transfers
to its condition.

## 7. Validation

Require exactly `schema: "marqdo.analysis"` and `schema_version: 3` before
traversing the document. The [JSON Schema](schemas/analysis.schema.json)
validates record shapes. Check source digests, spans, ownership and
reference targets separately. Its full branch and loop structure applies
equally to simple projects and imported modules.

Source digests cover the original bytes. Preserve UTF-8 byte offsets for
BOM, CRLF, CJK and emoji. Observation binds to the SHA-256 of the exact
emitted Analysis JSON bytes.
