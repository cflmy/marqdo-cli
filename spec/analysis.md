# Marqdo Analysis

Current public contract for `marqdo check --emit PATH`. Every project emits
`marqdo.analysis` with `schema_version: 3`. The contract is implemented by
[ADR-0069](../docs/adr/0069-public-analysis-boundary.md); it replaces the
program-only Analysis shape in place. Retired artifacts with the same revision
are rejected because every field below is required.

## 1. Invariants

`check --emit`:

* never executes the document, opens a plugin shared library, starts a worker
  or foreign kernel, accesses the network, or imports a module implicitly;
* validates the prepared program first; on a compile or load error it writes
  no artifact and only removes a previous artifact at the caller-specified
  output path when that path is a safe output (see §8);
* writes through an owner-only exclusive temporary file in the target directory
  and renames it into place atomically;
* contains no absolute machine paths, timestamps, PIDs, random ids or other
  environmental data, so the same logical project emits byte-identical
  Analysis from different checkout directories;
* exposes only facts that one of `check`, `inspect`, `run` or `call` already
  consumes through `project_prepare::prepare`. There is no second parser,
  second call-binding pass or tool-specific branching.

`source` is present because it is the one artifact that lets a reader slice
every span without knowing package-cache or stdlib layouts. It contains only
modules that the loader already read statically; no extra file, asset or
binary is read. Treat an emitted Analysis as sensitive local source data:
`--help` and this specification say so, and the CLI neither uploads nor logs
it.

`AnalysisDigest` is the lowercase hex SHA-256 of the exact UTF-8 bytes written
by `check --emit` (pretty JSON), represented in Observation as
`sha256:<64 hex>`. Diagnostics and every static option that changes the
projection are part of those bytes. `run`/`call` bind to the same projection
with the same static options; a `--refs` invocation adds diagnostics and
therefore has a different digest, and must not be substituted for the ordinary
artifact.

## 2. Top-level shape

```json
{
  "schema": "marqdo.analysis",
  "schema_version": 3,
  "producer": {
    "name": "marqdo",
    "version": "<product version>",
    "markdown_profile": "marqdo.md.v1"
  },
  "project": {
    "root_id": "root:demo@0.1.0",
    "identity_kind": "manifest",
    "entry_module_id": "root:demo@0.1.0::index.mq.md",
    "lock_digest": "sha256:<64 hex>",
    "manifest_digest": "sha256:<64 hex>",
    "stdlib": null,
    "extensions": []
  },
  "modules": [],
  "diagnostics": []
}
```

### 2.1 Project identity and snapshot

| Field | Meaning |
|---|---|
| `root_id` | `root:<name>@<version>` for a project with `marqdo.yaml`; `sha256:<64 hex>` content snapshot without one. A snapshot is not a persistent project identity. |
| `identity_kind` | `manifest` or `workspace_snapshot`. |
| `entry_module_id` | The canonical id of the module loaded from `ENTRY`; it must occur exactly once in `modules`. A second entry of the same project produces its own Analysis and is never merged by basename. |
| `lock_digest` | `sha256:` + SHA-256 of the exact `marqdo.lock` bytes captured during preparation; `""` only when no lock file exists. A read failure is a load error, never an empty digest. |
| `manifest_digest` | `sha256:` + SHA-256 of the exact `marqdo.yaml` bytes captured during preparation; `""` when there is no manifest. This distinguishes equal name/version projects with different static configuration. |
| `stdlib` | `null`, or the verified bundle `{version, digest, instance_id}` actually used by this load. |
| `extensions` | Static extension identities that participated in preparation, sorted by `(kind, name, logical_owner, manifest_digest)`. See §2.2. |

Manifest and lock bytes are read once while loading. Analysis never re-reads
them; a concurrent edit cannot make the artifact claim bytes other than the
snapshot that determined the program.

### 2.2 Extension identity

```json
{
  "kind": "plugin",
  "name": "example.echo",
  "version": "0.1.0",
  "logical_owner": "root:demo@0.1.0",
  "source": "./plugin",
  "manifest_digest": "sha256:<64 hex>",
  "artifact_digest": "sha256:<64 hex>",
  "selected_target": "x86_64-unknown-linux-gnu"
}
```

`kind` is `plugin`, `kernel` or `provider`. Declared extensions use the lock v4
identity that was validated during preparation; explicit `--plugin` descriptors
are included with their verified manifest identity, `source: "--plugin"` and
`logical_owner: "cli"`. No secret, physical path, handle, signature or grant is
projected. Two exports of the same package are one identity; equal export names
in different packages remain distinct.

### 2.3 Module paths and ordering

Modules are sorted by canonical `id`. `id` is
`<package instance>::<package-relative path>` and is scoped by
`project.root_id`. `relative_path` must be non-empty UTF-8 with `/`
separators, contain no empty, `.` or `..` segment and no OS absolute, drive or
UNC prefix. Valid names such as `foo..bar.mq.md` are accepted; a file name that
cannot be represented losslessly is rejected rather than silently rewritten.

## 3. Module shape

```json
{
  "id": "root:demo@0.1.0::index.mq.md",
  "package_instance_id": "root:demo@0.1.0",
  "relative_path": "index.mq.md",
  "source_digest": "<64 hex SHA-256 of source bytes>",
  "source": "<exact UTF-8 source text>",
  "document": {"kind": "document", "span": {"start": 0, "end": 0}, "children": []},
  "units": [],
  "anchors": [],
  "functions": [],
  "imports": [],
  "references": [],
  "calls": [],
  "resources": []
}
```

`source_digest` is the lowercase hex SHA-256 of the decoded UTF-8 bytes of
`source`. BOM, CRLF, missing trailing newline and NUL bytes are preserved in
`source`; JSON escaping changes the encoding only, not the decoded bytes.
Every `span` in the module indexes `source`.

### 3.1 Document AST

`document` is the public shape already defined by
`spec/markdown-profile.md` §5, emitted by an explicit projector and shared with
`inspect --ast`. Every node has `kind`, `span` and `children` (leaves use `[]`).
Leaves carry `literal` where the profile defines one. Attribute rules:

| Kind | Attributes |
|---|---|
| `document`, `block_quote`, `paragraph`, `strong`, `emph`, `strikethrough`, `table_cell` | none |
| `front_matter` | `literal` |
| `list` | `ordered`, `start` (`null` for bullet), `tight` |
| `item` | `task` (`null`, `true`, `false`) |
| `heading` | `level` (1-6), `setext` |
| `text`, `math_block`, `html_block`, `html_inline` | `literal` |
| `code` | `ticks` (delimiter length), `literal` |
| `code_block` | `fenced`, `info`, `literal` |
| `math` | `display`, `literal` |
| `link`, `image` | `url`, `title` |
| `table` | `alignments` (`none`, `left`, `center`, `right`) |
| `table_row` | `header` |
| `thematic_break`, `soft_break`, `line_break` | none |
| `other` | `tag`, optional `literal`, children preserved |

The root `document` span covers the whole source including a BOM. All node
spans are half-open UTF-8 byte offsets, `start <= end <= source.len()` and on
UTF-8 boundaries. Container spans follow the parser source map, including its
documented empty/synthetic exceptions; semantic validation checks the
invariants that hold and does not pretend `literal` is byte-identical to the
span (entities, escapes and code whitespace may be normalised). HTML, links and
code blocks are inert data.

### 3.2 Units and heading scopes

`units` is in document order with the root first:

```json
{
  "id": "root:demo@0.1.0::index.mq.md#力学/计算力",
  "path": ["力学", "计算力"],
  "parent_id": "root:demo@0.1.0::index.mq.md#力学",
  "kind": "function",
  "name": "计算力",
  "level": 2,
  "mode": "exec",
  "mode_origin": "explicit",
  "mode_source_unit_id": "root:demo@0.1.0::index.mq.md#力学/计算力",
  "heading_span": {"start": 10, "end": 30},
  "span": {"start": 10, "end": 90},
  "function": "root:demo@0.1.0::index.mq.md::力学.计算力"
}
```

| Field | Requirement |
|---|---|
| `id` | Canonical unit id. Display escapes `/`, `#` and `%` inside each path segment as `%2F`, `%23`, `%25`; `path` is the authoritative structure and consumers must not split `id`. |
| `path` | Canonical NFC/whitespace-normalised heading path, including this heading; `[]` for the root. |
| `parent_id` | Root: `null`. Non-root: nearest real ancestor unit, `null` only for the root; not the Markdown DOM parent. |
| `kind` | `root`, `namespace` (H1), `function` (Exec H2), `knowledge` (Knowledge H2), `section` (H3-H6). |
| `name` | Normalised heading name; `""` for the root. |
| `level` | 0 for the root, otherwise 1-6. |
| `mode` | Effective `exec` or `knowledge`; the root is `knowledge`. |
| `mode_origin` | Root: `default`. Non-root: `explicit` when the ATX heading declares `@exec`/`@knowledge`; otherwise `inherited`. Setext headings never declare. |
| `mode_source_unit_id` | The unit that provides the effective mode: the root, or the nearest explicit ancestor (possibly the unit itself). Must be the unit or one of its ancestors. |
| `heading_span` | Root: `null`. Otherwise the original heading AST span. |
| `span` | Root: whole source including BOM. Otherwise the complete section range: from the heading start to the next heading of the same or higher level, or EOF; it includes all child headings and excludes the next sibling. |
| `function` | Canonical owning `FunctionId` for an Exec section; `null` for Knowledge/namespace/root. Only an Exec H2 creates a callable; nested sections point at it. |

Mode declarations are only the ATX first-Text-child `@exec`/`@knowledge`
tokens; any other `@word` is `sem.invalid_mode_marker`. Mode inheritance pops
same-or-higher-level ancestors first, so a sibling never supplies a mode.
Headings inside lists, block quotes or fences do not create language units.

### 3.3 Anchors

```json
{"anchor": "计算力", "unit_id": "root:demo@0.1.0::index.mq.md#力学/计算力"}
```

`anchors` projects the single `UnitIndex.anchors` map, sorted by `anchor`.
Anchors are navigation names, not identities; slug collisions use the current
numbered algorithm. A display title, semantic name, anchor and unit id are
different concepts; the display title comes from the AST.

### 3.4 Functions

```json
{
  "id": "root:demo@0.1.0::index.mq.md::力学.计算力",
  "unit_id": "root:demo@0.1.0::index.mq.md#力学/计算力",
  "local_name": "计算力",
  "namespace": "力学",
  "span": {"start": 10, "end": 90},
  "heading_span": {"start": 10, "end": 30},
  "signature": {
    "kind": "explicit",
    "params": [{"name": "n", "required": true, "type": "Int", "default": null,
                "span": {"start": 31, "end": 41}}]
  },
  "effects": ["unknown"],
  "body": []
}
```

Every function corresponds one-to-one to an Exec H2 and to its `function` unit.
`namespace` is `null` outside an H1 namespace. Parameters keep declaration order
for explicit signatures and the canonical order for inferred ones; spans are
projected as written. `effects` is the honest conservative default
`["unknown"]`, never a purity proof.

`body` supports every current public statement: `assign` (string local target or
`{"kind":"index",...}`), `expression`, `branch` (ordered arms), `loop`,
`return`, `break` and `continue`, with the expression variants `literal`,
`variable`, `list`, `record`, `unary`, `binary`, `member`, `index`, `call`.
Arbitrary-precision integers are decimal strings; a float is a finite JSON
number. An unknown required kind is rejected by schema/semantic validation, not
silently dropped.

### 3.5 Imports

`imports` keeps `{alias, target, span, resolved}` in document order.
`resolved` is the canonical target module id from the single prepared import
resolution, or `null` for a diagnostic import. Imports are explicit; a link is
never an import.

### 3.6 References

See §5.

### 3.7 Calls

```json
{
  "function": "root:demo@0.1.0::index.mq.md::main",
  "span": {"start": 40, "end": 52},
  "callee": {"kind": "ident", "name": "计算力"},
  "state": "function",
  "target": "root:demo@0.1.0::index.mq.md::力学.计算力",
  "target_kind": "function",
  "target_owner": null,
  "reason": null
}
```

Calls are projected from the single prepared `CallBindingTable`, one record per
call expression occurrence, in source order. `state`:

| State | `target` | `target_kind` |
|---|---|---|
| `function` | existing canonical `FunctionId` | `function` |
| `builtin` | canonical builtin name | `builtin` |
| `plugin` | full dotted export name | `plugin` |
| `unresolved` | `null` | `null` |

For `plugin`, `target_owner` is
`{"kind":"plugin","package":"<package>","digest":"sha256:<64 hex>"}` from the
static descriptor; no plugin function node is fabricated. `reason` is
`dynamic` for a value call and `unbound` when preparation could not bind a
statically known callee, otherwise `null`. Legal dynamic calls do not
invalidate Analysis; genuine compile errors do.

### 3.8 Resources

`resources` projects exactly the inert `marqdo-ir::Resource` kinds `formula` and
`code`: `id` (module-local ordinal), `module_id`, `unit_id`, `kind`, `span`,
`content_digest`, `language` (`null` for formula), `display` (`true|false` for
formula, `null` for code). The payload lives in `source`, not in the artifact.
Ordinals are snapshot-local, never persistent symbols.

## 4. Diagnostics

```json
{
  "module": "root:demo@0.1.0::index.mq.md",
  "relative_path": "index.mq.md",
  "span": {"start": 10, "end": 20},
  "line": 2,
  "column": 3,
  "severity": "error",
  "code": "sem.invalid_mode_marker",
  "message": "...",
  "hints": []
}
```

`span` is primary; `line`/`column` are 1-based presentation derived from the
owning module and may be `null`. A successful Analysis contains no `error`
diagnostic. Diagnostics are deterministic: source position, then code.

## 5. Reference resolution

One pure resolver (`crates/cli/src/document_refs.rs`) handles every Markdown
link. It takes the source module, source span, raw URL and an explicit catalog
of documents with anchors and exact names. It performs no load, call binding,
import, network access or caller-entry fallback.

### 5.1 Catalog scope

* Ordinary Analysis uses only the modules already loaded for this program.
  A local target outside that set is `not_loaded`; the artifact states that the
  current scope did not verify it, never that it is absent.
* `check --refs` and `inspect --refs` build an explicit extension catalog:
  direct local `.mq.md` targets are read once and parsed with the same heading
  index; plain files are checked for existence only. They are not added to
  `modules`, imports, call bindings or the program, and are never executed.
  Budget/IO failures produce errors, never a silent downgrade.
* Given the same catalog, Analysis, `check` and `inspect` must produce the same
  state for the same link.

### 5.2 Reference record

```json
{
  "source": "root:demo@0.1.0::index.mq.md",
  "source_unit": "root:demo@0.1.0::index.mq.md#力学",
  "url": "./theory.mq.md#formula",
  "span": {"start": 20, "end": 50},
  "state": "not_loaded",
  "target_kind": null,
  "target_module": null,
  "target_unit": null,
  "candidate_package_instance_id": "root:demo@0.1.0",
  "candidate_path": "theory.mq.md",
  "fragment": "formula",
  "candidates": []
}
```

`source_unit` is the innermost unit whose complete `span` contains the link.
`candidate_path` is the normalised package-relative logical candidate; no OS
path is emitted. `fragment` is the once-decoded fragment or `null`; the raw
`url` is always preserved. `candidates` contains exact unit ids only for an
ambiguous canonical-name fallback.

| State | Meaning | Constraints |
|---|---|---|
| `resolved` | exact target in this catalog, or an explicitly verified local file | `target_kind: module` requires `target_module`; no fragment means `target_unit: null`; a verified plain file uses `target_kind: file` and needs no module |
| `not_loaded` | local candidate is outside this catalog; existence unverified | `target_unit: null`; never reported as missing |
| `missing` | catalog module has no such fragment, or explicit verification proved the file absent | keep the known `target_module` when available |
| `ambiguous` | canonical-name fallback has two or more exact candidates | `target_unit: null`; `candidates` lists all |
| `external` | URI has an external scheme (`http`, `https`, `mailto`, ...) | no network access; target fields `null`; not an error |
| `outside_scope` | local target escapes the allowed package root, or uses an absolute/file/drive/UNC path | target unread; no physical path emitted |
| `invalid` | malformed URI/percent encoding or unsupported address | no guessed repair |

Exact anchor lookup comes first; the existing canonical-name fallback (if
retained) collects all exact name candidates: 0 `missing`, 1 `resolved`, 2+
`ambiguous`. No `.find()`, case folding, edit distance or fuzzy binding. An
empty fragment is the document root. The fragment is percent-decoded exactly
once; invalid sequences are `invalid`.

Consumers may not reinterpret a non-`.mq.md` target as a Marqdo module. A
regular Markdown fragment is not verified against a foreign document profile:
the state is `not_loaded` with a diagnostic that coverage was insufficient, not
`missing`.

### 5.3 Security

Paths are normalised segment-by-segment after one percent decode. `file:`
URIs, OS absolute paths, drive/UNC paths, NUL, symlink escape and encoded
traversal are rejected or `outside_scope`. A legal `../` is allowed only while
the normalised candidate stays inside the package module root. The ordinary
Analysis opens no candidate. `--refs` opens only allowed local files, with
size/count limits, conservative symlink rejection and canonical root checking.

## 6. Validation

There are two steps on one boundary:

1. the [JSON Schema](schemas/analysis.schema.json) validates closed record
   shapes, required fields, enums, types and array element shapes;
2. a semantic validator checks cross-record invariants that JSON Schema cannot
   express.

The required semantic invariants are:

1. schema/revision/profile/entry/root/package/module identities agree; ids are
   unique; paths do not escape their package; text is valid UTF-8.
2. every `source_digest` matches its `source`; every span belongs to that
   module's source, has `start <= end <= source.len()` and lies on UTF-8
   boundaries; functions, statements, resources and references never point at
   another module.
3. there is exactly one root unit; parent edges form a tree (no cycle); each
   `path` equals its parent's path plus its name; full spans nest with their
   parent; heading AST, unit, function and anchor links exist and are
   unambiguous; mode source units are self-or-ancestor.
4. functions are unique and correspond to Exec H2 units; each `function` field
   points at the real definition; resolved call targets exist; builtin/plugin
   targets never pretend to be Marqdo functions; imports resolve to catalog
   modules.
5. reference state/nullability/candidates are consistent; `resolved` targets
   exist in the catalog; ordinary Analysis has no fabricated external module;
   resources refer to real units and their digests match the AST payload.
6. diagnostic owner/severity/code/span/line/column are valid; a successful
   Analysis has no error; all arrays and modules are deterministically sorted.

The producer runs typed local checks before encoding; the independent
`tests/consumers/analysis_consumer.py` and `validate_protocols.py` validate the
serialized bytes. Production validation never replaces consumer validation of
untrusted input.

## 7. Failure and exit semantics

`check` keeps its existing CLI contract: success `0`; static diagnostic
errors and verified lock/extension load failures `1`; argument errors, a
missing/unreadable entry document and output-publication failures `2`. This
matches the current implementation and the other commands; this contract change
does not reclassify a missing `ENTRY` as a static diagnostic. `--json` is a
check diagnostic summary, not a second Analysis writer. `--emit` is the only
full Analysis writer.

A failed check publishes no success artifact. It may remove a previous artifact
at the caller-specified output path only when that path is not an input source,
manifest, lock/dependency file, directory, symlink or hard link to one, and only
when that previous artifact is the one the caller named. Partial/unsuccessful
Analysis documents do not exist.

## 8. Output safety

`--emit PATH`:

* refuses a path equal to any loaded source, the manifest or the lock file,
  including through a hard link or symlink;
* refuses a directory, a symlink at the destination, or an output whose parent
  cannot be created safely;
* creates the temporary file exclusively with owner-only permissions on Unix,
  writes and flushes it, then renames it over the destination;
* on failure removes only its own temporary file and leaves inputs untouched;
* uses the same canonical encoder for the file and for the Observation digest.
  No second pretty writer is allowed.

## 9. Observation binding

`run`/`call` compute `source.analysis_digest` by encoding the same
`PreparedProgram` with the same encoder and hashing the exact bytes.
Observation never copies `source`, document AST or link targets into runtime
events. A run with different static options (for example `--refs`, `--plugin`
or a different entry) binds a different artifact; consumers must re-check
digests against the disk version they rely on.
