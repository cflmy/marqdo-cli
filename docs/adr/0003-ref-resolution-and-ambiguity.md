# ADR-0003: `ref` resolution order and `ref.ambiguous` (S-07)

Status: **Accepted**
Date: 2026-11-20

## Context

`ref("#anchor")` is the only way to turn a heading range into an inert
resource value (`Formula` or `Code`) or a `FunctionRef`. The blueprint
(§5.2, unresolved issue **S-07**) is explicit that a heading range can contain
several formulas or code fences, and that the implementation must **never
silently pick the first one**:

> 一个范围存在多段公式时，默认对象选择必须是明确规则（例如禁止无选择器，要求
> `math.formulas(ref(...))` 列出候选；不选"第一条"）。

The failing example is easy to write:

```markdown
## f

### 公式

$$a = 1$$

$$b = 2$$

**x = ref("#公式")**
```

If `ref` returned `a = 1`, the program would be non-deterministic with respect
to document structure (insert an earlier formula and the meaning changes) and
would hide a genuine authoring error.

At the same time the common case — one formula per heading — must stay simple;
requiring a selector for every reference would make `examples/math.mq.md`
awkward.

## Decision

`crates/marqdo-runtime/src/builtins_impl.rs::resolve_ref` implements this
algorithm:

1. The target must be a `Text` beginning with `#`; otherwise
   `symbol.unknown` ("must be a `#heading` anchor"). `ref` is not a URL
   resolver and does not cross modules.
2. The fragment is looked up in the **current module's anchor map**
   (`Module::anchors`, GitHub-style slugs computed in
   `crates/marqdo-frontend/src/units.rs`). `ref` does not fall back to
   canonical unit names and does not search imported modules.
3. Collect the resources attributed to the resolved unit (innermost unit by
   span, `crates/marqdo-frontend/src/resources.rs`).
   * **Exactly 1** → return `Formula` or `Code` (with `unit` and `span`
     provenance).
   * **0** → if a function's unit equals the resolved unit, return
     `FunctionRef { module, name }`; otherwise `symbol.unknown`.
   * **≥ 2** → return **`ref.ambiguous`**:
     "`#...` contains N resources; select one explicitly instead of relying on
     the first". Never choose a resource.
4. Function references are callable through the normal call path; formulas and
   code remain inert until `math.eval`/`foreign.*` is explicitly called (those
   are separately capability-checked).

Resource attribution is deterministic: a resource belongs to its innermost
enclosing heading unit, resources keep document order, and a resource is a
Markdown `math`/`math_block` or `code_block` node (inline code is not a
resource).

The `ref.ambiguous` category is `ErrorCategory::RefAmbiguous` with the stable
code `ref.ambiguous` (`crates/marqdo-value/src/error.rs`).

## Alternatives

1. **First resource wins** (blueprint's rejected baseline). Rejected by S-07:
   document edits would silently change program meaning, and there is no
   diagnostic for the author.
2. **Always require a selector argument**
   (`ref("#f", selector="second")`). Rejected for M0.5: no selector syntax or
   addressing scheme is specified yet, and the single-resource case is by far
   the common one. A future ADR can add an explicit selector while keeping the
   ambiguity rule.
3. **Error on every heading with ≥ 2 resources at compile time**, even if never
   referenced. Rejected: a document may legitimately contain several formulas
   for prose and never `ref` them; only the reference is ambiguous.
4. **Return a list of all candidates.** Rejected for v1: it changes the return
   type and forces every caller to handle a selector; a dedicated API
   (`math.formulas(ref(...))`) would be a new spec, not a silent widening.
5. **Cross-module `ref`.** Rejected for v1: the wire/identity model and
   `ModuleId` resolution live in the loader; `ref` is a pure runtime builtin
   over the current module. Cross-module resources are an open item.

## Markdown AST shape

```text
heading(level=2) "f"
  heading(level=3) "公式"
    math_block "/$$ a = 1 $$/"     -> Resource::Formula { unit: <#f/公式> }
    math_block "/$$ b = 2 $$/"     -> Resource::Formula { unit: <#f/公式> }
  paragraph -> strong("x = ref(\"#公式\")")
```

Anchors are separate from unit paths: `#公式` is a GitHub slug for the heading
name, while the canonical `UnitId` is `<module>#f/公式`. Colliding slugs get
`-1`/`-2` suffixes; canonical ids never do.

## Compatibility

* `ref.ambiguous` is a stable runtime code; callers may branch on it.
* The single-resource and function-reference cases are frozen. Adding a
  selector must be additive (new argument or new builtin) and must not change
  the ≥ 2 error into a silent choice.
* `ref` target syntax (`#fragment`) is frozen; changing it to accept file paths
  would need a new ADR.
* No package/ABI/wire change: formula/code values already carry
  `unit`/`span`/`content_hash`.

Example behavior, verified by
`crates/marqdo-runtime/tests/interpreter.rs::ambiguous_ref_resource_is_an_error`:

```text
$ marqdo run index.mq.md   # heading with two $$...$$ blocks and ref("#公式")
runtime error[ref.ambiguous]: `#公式` contains 2 resources; select one explicitly instead of relying on the first
```

## Security

* `ref` is a pure builtin: no host call, no capability required, no side
  effect. It only returns inert values or a function reference.
* An inert `Formula`/`Code` does not execute. Evaluation/execution requires a
  separate explicit call (`math.eval`, `foreign.run/call`), which is capability
  checked at the host boundary.
* Refusing ambiguity prevents a class of "wrong formula executed" mistakes,
  which matters for scientific documents where formulas are evidence.

## Testing

* `crates/marqdo-runtime/tests/interpreter.rs::ambiguous_ref_resource_is_an_error`
  drives a real document through frontend + runtime and asserts the
  `ref.ambiguous` code.
* `crates/marqdo-frontend/src/resources.rs` tests attribution to innermost
  functions, multiple resources kept in one unit, and stable SHA-256 code
  hashes.
* `crates/marqdo-markdown`/`frontend` tests cover anchors and slug collisions.
* `examples/math.mq.md` is the positive single-formula case; it runs under
  `marqdo run` only via a function (the example itself defines callable
  functions, no `main`).
* Missing anchor/not-a-function cases are covered by the `symbol.unknown`
  paths; conformance fixtures are being added by the fixtures workstream.

## Migration & rollout

No migration needed for documents with exactly one resource per referenced
heading. Documents that relied on "first formula" behavior (there are none in
the examples) must split headings or wait for an explicit selector API. If a
selector is added later, keep `ref("#...")` behavior unchanged and introduce
the selector as a new argument/builtin, with tests for both the ambiguous and
the selected case.
