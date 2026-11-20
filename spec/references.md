# Marqdo References, Imports and Calls v1

Status: **frozen for M0.5 except where marked "current limitation".** Language
changes require an ADR plus updated tests.

Normative implementation:

| Concern | Code |
|---|---|
| Front matter parsing | `crates/marqdo-markdown/src/frontmatter.rs` |
| Module loading, project root, cycles | `crates/cli/src/loader.rs` |
| Module/Unit/Function identities | `crates/marqdo-ir/src/lib.rs` |
| Heading scopes, anchors | `crates/marqdo-frontend/src/units.rs` |
| Resource collection and attribution | `crates/marqdo-frontend/src/resources.rs` |
| Link-call mapping | `crates/marqdo-frontend/src/map.rs` |
| `ref(...)` runtime resolution | `crates/marqdo-runtime/src/builtins_impl.rs` |
| Static link graph (`inspect --refs`) | `crates/cli/src/commands.rs` |
| Cross-module call validation | `crates/marqdo-frontend/src/calls.rs` |

This file complements [`mapping.md`](mapping.md) (AST roles) and
[`package-format.md`](package-format.md) (manifest/lock file).

## 1. Three edge kinds, never conflated

Blueprint §4.1 distinguishes three relations. The implementation keeps them
separate and **only the call edge can execute code**.

| Edge | Written as | Resolved when | Executes? |
|---|---|---|---|
| **Import** | YAML front matter `import:` mapping | module load / static analysis | No. Imported modules only register definitions; their `main` never runs. |
| **Reference** | Markdown `[label](url)` | `inspect --refs` (static graph) | No. Rendered/read as documentation only. |
| **Call** | `> f(...)` block quote, link-form block quote `> [f](#f)(...)`, or a `CallExpr` in an executable payload | static validation, then execution | Yes, at runtime, through the interpreter. |

Consequences:

* A Markdown link never calls, imports or loads anything. `[计算作用力](#计算作用力)`
  is a reference; `> [计算作用力](#计算作用力)()` is a call.
* `ref("#anchor")` is a **runtime builtin**, not an import; it only resolves an
  inert resource or function reference in the running module.
* The reference graph is static metadata and creates no proof obligation.

## 2. Canonical identities

These types live in `crates/marqdo-ir/src/lib.rs`.

### 2.1 `ModuleId`

```rust
pub struct ModuleId(pub String);
```

* A canonical **project-root-relative path with `/` separators**, for example
  `index.mq.md`, `packages/geometry/index.mq.md`.
* Computed in `crates/cli/src/loader.rs::Loader::module_id_for` by taking the
  canonicalized file path, stripping the project root, and joining the path
  components with `/`.
* There is no "module name" identity separate from the path: the module id is
  the file location inside the project. Imported `@dependency` modules keep
  their project-root-relative path (for example
  `packages/geometry/index.mq.md`).
* A file that canonicalizes outside the project root is rejected with
  "outside the project root" (entry: fatal load error; imported module:
  `module.unresolved` diagnostic).
* Module ids use `BTreeMap` ordering, so program iteration and JSON output are
  deterministic.

### 2.2 `UnitId`

```rust
pub struct UnitId { pub module: ModuleId, pub path: Vec<String> }
```

* One addressable heading range. `path` is the canonical heading path from the
  outermost heading to this unit (root unit has an empty path).
* Canonical heading names are NFC-normalized, trimmed, with internal
  whitespace collapsed to one space (`marqdo_markdown::normalize_name`).
* `Display` renders `<module>#<path joined by '/'>`; the root unit renders as
  `<module>#`.
* `UnitKind` is `root`, `namespace` (`#`), `function` (`##`) or `section`
  (`###`+). Sections are addressable but not callable.

### 2.3 `FunctionId`

```rust
pub struct FunctionId { pub module: ModuleId, pub name: String }
```

* `name` is the **namespace-qualified canonical function name**: `ns.fn` when
  the level-2 heading is inside a level-1 namespace, otherwise `fn`
  (`crates/marqdo-frontend/src/units.rs`). `Display` is `<module>::<name>`.
* A function heading under a namespace yields both `FunctionId.name` (`ns.fn`)
  and `Function.local_name` (`fn`) plus `namespace: Some("ns")`.
* Lookup (`Module::function`) first matches `id.name`, then a unique
  `local_name`; if several namespaces define the same local name the lookup
  returns `None` rather than guessing.
* No overloads: duplicate canonical names in one scope are hard errors
  (`markdown.invalid_construct`, "duplicate heading name ... in the same
  scope") and are never auto-suffixed.

### 2.4 Anchors are a separate namespace, not identity

`UnitIndex.anchors: BTreeMap<String, UnitId>` maps GitHub-style heading slugs
to units (`crates/marqdo-frontend/src/units.rs::github_slug`):

* digits/letters/`_` are kept and lowercased; spaces, `-` and tabs become `-`;
  all other punctuation is dropped;
* collisions receive `-1`, `-2`, ... suffixes in document order (separate
  counters per base slug);
* anchors exist for navigation and `ref("#...")`; they are **not** identity.
  Internal ids stay canonical, so a heading rename is a link-breaking change
  but never changes the canonical unit path.

## 3. Front matter imports

Imports exist only as an explicit YAML mapping in the document's front matter
(recognized by the Markdown profile only at the start of the file):

```markdown
---
import:
  geometry: "@geometry"
  util: "./lib/util.mq.md"
---

# main
```

Parsing rules (`crates/marqdo-markdown/src/frontmatter.rs::imports`):

* the `import` key must be a mapping of **string alias → string target**;
* aliases must be non-empty and unique within the document;
* any other front matter key is metadata for tools and is ignored by the
  interpreter; invalid YAML is a `frontmatter.invalid` diagnostic, never
  silently dropped;
* imports are never inferred: a link, a file name or a directory scan does not
  create an import;
* the diagnostic span recorded for all imports of a file is the whole front
  matter span (`ImportDecl.span`).

### 3.1 Target forms

`Loader::resolve_import` recognizes exactly two forms:

| Target | Resolution |
|---|---|
| `@dependency` | Look up the alias after `@` in `marqdo.yaml` `dependencies`. The dependency must declare a **path** source. Resolve `project_root / path`; if it is a directory, append `index.mq.md`; canonicalize. A git-only dependency cannot be resolved in this build (`dependency.path` is `None` → unresolved). |
| any other string (intended: `./file.mq.md`) | Join the string to the **importing file's directory**, then canonicalize. There is no extension inference, no search path and no fallback to a package directory. |

The resulting file is loaded as a module. If it canonicalizes outside the
project root, the loader reports "outside the project root" and the import
becomes a `module.unresolved` diagnostic.

### 3.2 Loader rules

`crates/cli/src/loader.rs` implements a static, non-executing load:

1. **Project root discovery** (`marqdo_package::discover_root`): walk up from
   the entry file (starting at its parent directory) to the nearest directory
   containing `marqdo.yaml`; if none is found, the entry file's parent
   directory is the root. There is no `--project` flag in this build and nested
   project boundaries are not diagnosed — the nearest manifest wins (open
   item).
2. **Manifest** is optional at run time; if a `marqdo.yaml` exists in the root
   it is parsed and validated. An invalid manifest is a fatal load error
   (exit code 2), not a diagnostic list.
3. **Entry module** is loaded first; `Program.main_module` is set to its
   module id.
4. **Imports** are processed depth-first in front matter order. Each import
   target is resolved as in §3.1, then loaded recursively.
5. **Cycle detection** uses the path stack: if the canonical target path is
   already being loaded, the importing file gets a `module.cycle` diagnostic
   ("import ... creates an import cycle") and the import edge is skipped.
   Modules are otherwise loaded once; a second import of an already-loaded
   module reuses it.
6. **Definitions only**: loading parses, indexes and analyzes source. No
   function is called, no `main` is run, no foreign/plugin code is touched.
7. After loading, `calls::validate_program` validates cross-module and
   qualified-builtin calls and attaches diagnostics to the emitting file.
8. Diagnostics are collected in deterministic (load) order; the CLI renders
   them on stderr (or as JSON).

Failure mapping: a failure to load the entry file or read the manifest is a
**fatal load error** (`exit 2`, `error: ...` on stderr). A failure to resolve
an imported module becomes a `module.unresolved` **diagnostic** on the
importing file and `check`/`run` then exit 1 (run refuses to execute a project
with compile errors).

## 4. Markdown reference edges (`inspect --refs`)

`marqdo inspect <file> --refs` walks every `link` node in the entry document
and prints/writes an edge array (`crates/cli/src/commands.rs::refs_json`,
`resolve_link`). Each edge has:

```json
{
  "source": "references.mq.md",
  "url": "./math.mq.md#计算圆面积",
  "span": {"start": 54, "end": 95},
  "resolved": true,
  "target_module": "math.mq.md",
  "target_unit": "math.mq.md#数学/计算圆面积"
}
```

Resolution rules:

1. `#fragment` → look up the entry module's anchors, then fall back to a unit
   whose canonical **name** equals the fragment.
2. `http://` / `https://` URL → counted as resolved with no local target.
   External links are never downloaded, imported or executed.
3. anything else → split optional `#fragment`, canonicalize the path relative
   to the **entry file's directory**, require the target to be inside the
   project root, derive its `ModuleId`, and look up anchors/names in that
   module. If the target module is not imported, its source is **read and
   analyzed statically on demand** (`analyze_source`) so broken anchors can be
   detected without execution.
4. `resolved: false` means the file or anchor could not be resolved.

Current limitations (documented, not hidden):

* `check --refs` validates every loaded document's link nodes and reports
  broken same-document anchors, missing files and missing target anchors as
  `ref.unresolved` errors, changing the command's exit status. Plain `check`
  (without `--refs`) does not treat reference problems as errors, because links
  are static knowledge references rather than execution inputs.
* `inspect --refs` prints the edge graph for the entry document only; other
  loaded modules are still covered by `check --refs`.
* The graph is not emitted for call edges or import edges; imports are visible
  through module counts/`inspect` summaries, calls through `inspect --ir`.

## 5. `ref("#anchor")` runtime semantics

`ref` is a **pure builtin** (`crates/marqdo-ir/src/builtins.rs`,
`crates/marqdo-runtime/src/builtins_impl.rs::resolve_ref`). It never evaluates
the referenced resource and requires no capability.

Signature: `ref(target: Text) -> Formula | Code | FunctionRef`.

Algorithm:

1. `target` must start with `#`; otherwise `symbol.unknown` with the message
   "ref target ... must be a `#heading` anchor". It is not a URL resolver.
2. The fragment is looked up **only in the running module's anchor map**
   (`Module::anchors`). In this build `ref` does not fall back to canonical
   unit names and does not search imported modules; a fragment that is not a
   GitHub slug yields `symbol.unknown` ("no heading matches ...").
3. The matching unit's resources (`Module::resources_of`, computed from the
   unit's heading range) are examined:
   * **exactly one resource** → `Formula { latex, display, unit }` or
     `Code { language, source, content_hash, unit }`. `unit` is the
     `UnitId::Display` string.
   * **zero resources** → if a function's unit equals the resolved unit, return
     a `FunctionRef`; otherwise `symbol.unknown`.
   * **more than one resource → `ref.ambiguous`** with the message
     "`#...` contains N resources; select one explicitly instead of relying on
     the first". The runtime **never** picks the first resource. This is
     blueprint issue S-07 and is frozen by ADR-0003.
4. Resource attribution (`resources.rs`): a formula or code fence belongs to
   the **innermost enclosing heading unit** by source span; resources are kept
   in document order. Markdown `math`/`math_block` and `code_block` nodes
   become resources; inline `code` spans do not.

Example (from `examples/math.mq.md`):

```markdown
## 圆面积公式

$$
A = \pi r^2
$$

## 计算圆面积

**面积 = math.eval(ref("#圆面积公式"), {"r": 4}, target="A")**

*面积*
```

`ref("#圆面积公式")` resolves the single formula under that heading; the
formula is still inert until `math.eval` consumes it.

### 5.1 Error model of `ref`

| Situation | Runtime code |
|---|---|
| target not starting with `#` | `symbol.unknown` |
| no anchor/unit matches | `symbol.unknown` |
| unit has no formula/code/function | `symbol.unknown` |
| unit has ≥ 2 resources | `ref.ambiguous` |
| module not present in the program | `symbol.unknown` |

`ref.unresolved` (`crates/marqdo-value/src/diagnostic.rs`) is a **compile-time**
code used by link-form call mapping, not by the `ref` builtin.

## 6. Call edges

### 6.1 Expression calls

Inside a function body, `CallExpr` nodes call:

* a local function (same module, namespace-aware resolution: current namespace
  first, then canonical name, then a unique local name);
* an imported function through its alias: `geometry.圆面积(r=2)`;
* a builtin (`打印`, `math.eval`, ...) by canonical name or alias;
* a `FunctionRef` value obtained earlier (for example from `ref`).

Static validation (`crates/marqdo-frontend/src/calls.rs`) checks arity,
argument names and the named-only rule for inferred signatures. Type errors are
runtime concerns in v1.

### 6.2 Block-quote calls

`> f(...)` is a call statement; the payload must parse as a complete
`CallExpr` (see `mapping.md` §4.5). Its result is discarded.

Link form `> [label](#anchor)(args)` resolves the link target **within the
current module** using the anchor map, then the canonical unit-name fallback,
and requires the target to be a function unit. Notes:

* a zero-argument link call must still write `()`; proxy `> [f](#f)` is a hard
  error;
* `http(s)` link targets used as calls are a hard `ref.unresolved` error;
  external links are never downloaded or called;
* a relative-path link target such as `> [f](./other.mq.md#f)(x=1)` is
  **not supported** in this build: mapping reports `module.unresolved` with
  the hint to import the module in front matter and call through its alias.
  Cross-module calls therefore go through an alias expression
  (`geometry.f(...)`), not through a Markdown link. Blueprint §2.4's
  `> [计算面积](./math.mq.md#计算面积)(r=3)` example is a **known divergence**.

### 6.3 Imported alias calls

At runtime an alias call resolves via `Program::resolve_import` to the target
module id; an unresolved alias is `symbol.unknown`. The static validator
checks the target module exists and the function/arity match when the whole
program is loaded, and emits `symbol.unknown` for an unknown function.

## 7. Diagnostics for references/imports

| Code | Severity | Where produced |
|---|---|---|
| `module.unresolved` | error | loader (bad import target), call mapping (relative-path link call) |
| `module.cycle` | error | loader |
| `ref.unresolved` | error | link-form call target mapping, external links used as calls, `check --refs` link validation |
| `ref.ambiguous` | runtime error | `ref` builtin (≥ 2 resources) |
| `symbol.unknown` | error / runtime error | static call validation; `ref` anchor miss at runtime |
| `frontmatter.invalid` | error | analyser (bad YAML/import shape) |
| `markdown.invalid_construct` | error | empty/duplicate heading names, invalid link-call shape |

## 8. Current limitations (open ADR candidates)

1. `ref("#...")` is module-local and has no cross-module form; there is no
   documented syntax for referencing a resource in an imported module.
2. Ordinary Markdown link resolution is inspection-only; `check` does not
   validate links and there is no `--refs` flag on `check`.
3. Link-form calls cannot cross modules (see §6.2).
4. Headings inside container blocks or fenced code never create units, and a
   heading whose name is empty is a hard error rather than an ignored heading.
5. Anchor lookup in `ref` does not fall back to canonical unit names, while
   link-form calls and `inspect --refs` do. This asymmetry is intentional for
   the frozen build but should be unified by a future ADR (it is easy to get
   surprising results with headings containing punctuation).
