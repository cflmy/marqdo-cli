# Marqdo Analysis v1

Status: **superseded by [Analysis v2](analysis-v2.md); never released, do not
write v1.** Kept as history. This was the M0.6 R4 read-only projection; it is
not a dump of `marqdo-ir`.

## 1. Invariants

`check --emit`:

* never executes the document, loads a plugin shared library, starts a foreign
  kernel or imports a module implicitly;
* validates the loaded program first; on a compile error it writes no artifact
  and removes a stale file at the target path;
* writes atomically (temporary file + rename) only after a successful check;
* is deterministic: the same source, tool version and project path produce
  byte-identical JSON (no timestamps, no random ids);
* exposes no private Rust layout, optimization instruction or host pointer.

## 2. Top-level shape

```json
{
  "schema": "marqdo.analysis",
  "schema_version": 1,
  "producer": {"name": "marqdo", "version": "0.1.0", "markdown_profile": "marqdo.md.v1"},
  "project": {"root_id": "sha256:...", "lock_digest": "sha256:..."},
  "modules": [],
  "diagnostics": []
}
```

* `root_id` is a digest of the normalized project root used for this run.
* `lock_digest` is a digest of `marqdo.lock`, or `""` when absent.
* Modules are sorted by canonical `id`; arrays keep source order.
* `schema_version` is a major version. Additive optional fields are allowed;
  removing/renaming fields or changing units/types requires a major bump.

## 3. Modules

```json
{
  "id": "root:default::index.mq.md",
  "package_instance_id": "root:default",
  "relative_path": "index.mq.md",
  "source_digest": "<sha256 hex>",
  "units": [],
  "functions": [],
  "imports": [],
  "references": [],
  "calls": [],
  "resources": 0
}
```

Module identity is the package-aware `ModuleId` from
`spec/references.md` §2.1. `source_digest` hashes the exact source bytes.

### 3.1 Units

Each unit has `id`, `kind` (`root`/`namespace`/`function`/`section`), `name`,
`level`, `span` (`{start,end}` byte offsets) and optionally `function`.

### 3.2 Functions and signatures

```json
{
  "id": "root:default::index.mq.md::数学.阶乘",
  "unit_id": "root:default::index.mq.md#数学/阶乘",
  "local_name": "阶乘",
  "namespace": "数学",
  "span": {"start": 0, "end": 100},
  "heading_span": {"start": 0, "end": 10},
  "signature": {
    "kind": "inferred",
    "params": [{"name": "n", "required": true, "type": null, "default": null, "span": {"start": 0, "end": 0}}]
  },
  "effects": ["unknown"],
  "body": []
}
```

`kind` is `explicit` or `inferred`. Inferred signatures only accept named
arguments. `effects` is a **conservative** summary; `unknown` is not a proof of
purity. Dynamic ABI/Foreign targets remain unknown unless a descriptor says
otherwise.

### 3.3 Normalized body nodes

Body nodes use a small stable language, never internal IR serialization:

* `literal` (`type`: `int`/`float`/`text`/`bool`/`none`, `value` when present);
* `variable`, `list`, `record`, `member`, `index`;
* `unary` (`not`/`neg`/`pos`) and `binary` (`or`, `and`, `eq`, `ne`, `lt`,
  `le`, `gt`, `ge`, `add`, `sub`, `mul`, `div`, `floor_div`, `mod`, `pow`);
* `call` with `callee` and `args`;
* `assign`, `expression`, `call_site`, `branch` (ordered arms), `loop`,
  `return`. (`call_site` existed for the pre-RFC link-form block-quote call and
  is not produced by the current language; this document is historical.)

Every node carries a byte `span`. Nodes outside the supported public subset
must be marked `unsupported` (future extension), never silently given invented
semantics.

### 3.4 Imports, references and calls

* `imports[]`: `{alias, target, span, resolved}`; `resolved` is a module id or
  null. Imports stay explicit.
* `references[]`: Markdown link edges with `{source, url, span, state,
  target_module, target_unit}`. `state` is `resolved`, `unresolved` or
  `external`; a failed resolution is never disguised as bound.
* `calls[]`: `{function, span, callee, state, target}` where `state` is
  `function`, `builtin`, `plugin` or `unresolved`.

Reference edges never execute anything and grant no capability.

## 4. Diagnostics

`diagnostics[]` uses the same stable codes/JSON shape as `marqdo check --json`.
An analysis emitted after a successful check normally has no error diagnostics
(warnings may remain).

## 5. Consumer contract

An independent consumer (Python/Go/JavaScript, without linking Rust crates)
must be able to:

* find a function by `id` and read its signature/body;
* find recursive/forward calls and `ref` edges;
* map every body node back to `source_digest` + byte span;
* reject `schema_version` it does not understand.

`tests/consumers/analysis_consumer.py` is one such black-box consumer and
`crates/cli/tests/analysis_trace.rs` runs it when Python is available.

## 6. Not included

Internal optimization IR, host pointers, plugin handles, proof status and
execution results are deliberately absent. Analysis does not claim that a
document runs successfully or that any function is proved.
