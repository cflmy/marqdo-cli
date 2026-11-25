# Public Analysis consumers after issue #10

Applies to `marqdo.analysis` revision 3 as defined by
[ADR-0069](../adr/0069-public-analysis-boundary.md), [spec/analysis.md](../../spec/analysis.md)
and [the schema](../../spec/schemas/analysis.schema.json). This page is the
integration guide for downstream static tools, in particular `marqdo-lint` and
`marqdo-view`.

The revision number did not change, but the shape did. An old revision-3
artifact is **not** accepted: current required fields are missing, and the
current schema and consumer semantics reject it. Rebuild the artifact and
update the consumer; do not add a compatibility reader or a second Analysis
shape.

## 1. What the CLI already provides

Run one command per program entry:

```sh
marqdo check path/to/index.mq.md --emit analysis.json
```

The artifact contains everything a document/lint/knowledge reader needs, from
the same static preparation as `check`, `inspect`, `run` and `call`:

- exact UTF-8 `modules[].source` and its SHA-256 `source_digest`;
- the Markdown AST in `modules[].document`;
- heading scopes in `modules[].units` and anchors in `modules[].anchors`;
- functions, imports, calls and resources;
- Markdown references with explicit resolution states;
- project context: `entry_module_id`, `identity_kind`, `root_id`,
  `manifest_digest`, captured `lock_digest`, stdlib and extension identities;
- deterministic diagnostics.

`inspect --ast` and `inspect --units` now project the **same** AST and unit
shape. They remain human/debug selections; a machine consumer should read
`check --emit` and stop running a second `inspect --ast` process per module.

## 2. Required consumer changes

### 2.1 Read the source from the artifact

`modules[].source` is the exact text the loader read. Every span is a half-open
UTF-8 byte offset into that string.

- Do not reopen the file from `relative_path` for spans or digests; package
  caches, stdlib and dependencies have no stable consumer-facing physical path.
- Verify `sha256(source.encode("utf-8")) == source_digest` before trusting a
  span.
- `relative_path` remains package-relative and useful for display and project
  layout, not for reconstructing an absolute path.

### 2.2 Document AST

`modules[].document` is the public AST described by
[`spec/markdown-profile.md`](../../spec/markdown-profile.md). Every node has
`kind`, `span` and `children` (`[]` for leaves); attributes/literals follow the
profile. The root `document` span now covers the whole source, including a BOM.

- Parse `document` directly; do not parse Markdown again for program
  annotations.
- Renderers may keep their own rendering parser, but any *semantic* decision
  (executable region, heading scope, function ownership) must use Analysis.
- Literal payloads are not guaranteed byte-equal to their span (entities,
  escapes and code whitespace are normalised).

### 2.3 Heading scope: `span` changed meaning

For every non-root unit:

| Field | Meaning |
|---|---|
| `heading_span` | The original heading AST node span. Use this to locate the heading itself. |
| `span` | **Complete section range** from the heading through the next heading of the same or higher level, or EOF. It includes child headings. |
| `path` | Canonical structured heading path. Do not split `id` to recover it. |
| `parent_id` | Nearest real ancestor unit; root is `null`. |
| `mode` | Effective `exec` or `knowledge`. |
| `mode_origin` | `explicit` (ATX declaration), `inherited`, or `default` for the root. |
| `mode_source_unit_id` | The unit that supplied the mode; self-or-ancestor. |
| `kind` | `root`, `namespace`, `function`, `knowledge`, `section`. |
| `function` | Owning function id for Exec sections; `null` otherwise. |

Do not infer section ends from the next heading's text or from the old
short-span assumption. Use `heading_span` for heading UI; use `span` for the
whole section body.

`anchors` is `[{anchor, unit_id}]`, sorted by anchor. Anchors are navigation
names, not identities; a missing anchor is not a missing document.

### 2.4 References: boolean `resolved` is gone

`modules[].references[]` now always carries:

```json
{
  "source": "root:x@0.1.0::index.mq.md",
  "source_unit": "root:x@0.1.0::index.mq.md#数学",
  "url": "./theory.mq.md#formula",
  "span": {"start": 20, "end": 50},
  "state": "not_loaded",
  "target_kind": null,
  "target_module": null,
  "target_unit": null,
  "candidate_package_instance_id": "root:x@0.1.0",
  "candidate_path": "theory.mq.md",
  "fragment": "formula",
  "candidates": []
}
```

| `state` | Consumer meaning |
|---|---|
| `resolved` | Exact module/unit or explicitly verified local file. `target_kind` is `module` or `file`. |
| `not_loaded` | The loaded catalog did not cover the local candidate. It is **not** missing; do not auto-create it. |
| `missing` | A known catalog module lacks the fragment, or explicit `--refs` verification proved absence. |
| `ambiguous` | Multiple exact canonical-name candidates; `target_unit` is null and `candidates` lists them. Never choose the first. |
| `external` | External URI scheme. No network access; not an error. |
| `outside_scope` | Absolute/drive/UNC/file URI, or a package-root escape. Never read it. |
| `invalid` | Malformed URI/percent encoding/unsupported address. |

Ordinary Analysis resolves only the loaded program. If a consumer needs to
verify unimported local files, use the CLI's explicit check:

```sh
marqdo check entry.mq.md --refs --json
marqdo inspect entry.mq.md --refs --json
```

Do not use the `--refs` artifact as the Observation binding input: `--refs`
adds diagnostics and changes the exact bytes. Run/call bind to the ordinary
`check --emit` bytes with the same static options.

### 2.5 Calls, functions and resources

- `functions[]` still has `id`, `unit_id`, `local_name`, `namespace`, `span`,
  `heading_span`, `signature`, `effects`, `body`.
- `calls[]` now has `target_kind`, `target_owner` and `reason`. A plugin target
  names the static owner (package/digest); it is never a Marqdo function.
  `unresolved` with `reason: dynamic|unbound` is honest, not an error.
- `resources[]` carries `display: null` for code and a boolean for formulas;
  verify its `content_digest` against the AST payload if you need integrity.
- The schema is closed: unknown required kinds/fields must be rejected, not
  ignored.

### 2.6 Project context and freshness

- Load `project.entry_module_id`; it must occur once in `modules`.
- `root_id` is a logical manifest identity or a content snapshot, not a
  cross-project permanent id.
- `manifest_digest` and `lock_digest` are the captured bytes; a changed file on
  disk means a new check, not a retry of the old artifact.
- Verify the exact bytes with the published JSON Schema + semantic checks; keep
  rejecting stale artifacts whose `source_digest` no longer matches the source
  you display.

## 3. `marqdo-view` integration

Current View behavior to replace:

1. It runs `check --json --emit` and separately calls `inspect --ast --json`
   per document. Replace the machine path with one `check --emit`; the AST is
   `modules[].document`. Keep `inspect` only for human debug output.
2. It re-reads files and compares `source_digest`. Prefer `modules[].source`
   directly; if it must use its own file snapshot, compare the artifact digest
   and reject a mismatch.
3. Its `RawProject` ignores new required fields. Make
   `entry_module_id`, `manifest_digest` and `extensions` required (or validate
   with the schema) and remove the v2 acceptance branch. View must accept only
   the current revision-3 shape.
4. `RawUnit.span` now means the complete section. For annotation placement use
   `heading_span`; for source highlighting use the complete `span`.
5. `RawReference` already reads `state`; remove any remaining `resolved`
   boolean mapping and map `not_loaded` as *unverified*, not as a broken link.
   For a local candidate the View catalog does not know, show "未加载/未核验";
   only `missing`/`ambiguous`/`invalid`/`outside_scope` are actionable errors.
6. Keep validating `source.analysis_digest` for Observation binding. The digest
   remains SHA-256 of the exact `--emit` bytes; `--refs` diagnostics change it.

Suggested request/render flow:

```text
check --emit -> verify schema + source_digest -> document AST
             -> units (complete span + heading_span) for annotations
             -> references (state) for link presentation
             -> calls/functions for executable markers
run/call + observation -> verify analysis_digest == sha256(artifact bytes)
```

## 4. `marqdo-lint` integration

Current Lint behavior to replace:

1. It performs one `inspect --ast` per module and then matches it with
   Analysis. Use `modules[].source` and `modules[].document` from the same
   `check --emit`; this removes a second process and any AST/span drift.
2. Its `AnalysisModule` model only requires the old fields and silently ignores
   the new ones. Make `source`, `document`, `anchors` and the new unit fields
   required; reject old same-revision artifacts. Prefer generating types from
   the published schema rather than `Vec<Value>` for units/references.
3. Heading-boundary rules must use `units[].span` as the complete section and
   `heading_span` as the heading itself. This is the authoritative range; do
   not infer boundaries from Markdown siblings.
4. Link/reference lint: iterate `references[].state`. Report:
   - `missing`/`ambiguous`/`invalid`/`outside_scope` as errors with
     `source_unit` + `span`;
   - `not_loaded` as an honest "unverified" notice, not a broken reference;
   - `external` as informational.
   Use the explicit `check --refs --json` run only when the user asks to verify
   direct unimported targets.
5. `calls[]` can report static call targets and dynamic/unbound reasons; do not
   infer a target from the display string. Plugin/builtin targets are not
   Marqdo functions.
6. Continue using `--json` for diagnostic summaries; `--emit` is the only full
   Analysis producer.

Suggested lint flow:

```text
check --emit -> verify schema + semantic invariants
             -> per module: document + units(full span/heading_span)
             -> rules: narrative/executable boundaries, calls, links, docs
             -> diagnostics projected from Analysis + local rules
```

## 5. Shared rules for both consumers

- One current contract. Do not add a v2/v3 reader, field probing, fallback AST
  from a second parser, or `try old shape then new shape`.
- Reject malformed/old artifacts with a corrective message: regenerate with the
  current CLI, reinstall the Tool manifest, rebuild the consumer.
- Tool manifests keep `requires.analysis: 3`; the revision is unchanged, so
  reinstall/rebuild is required because the old same-revision bytes are
  rejected by the current schema.
- No private Rust crate, no `inspect --ir`, no package-cache path, no writing
  to the analyzed project.
- The source in Analysis may contain secrets from the document. Do not log or
  upload it by default; store artifacts with owner-only permissions.
- Keep the three independent fixture flows green: document rendering, lint with
  diagnostics, and multi-entry knowledge linking.

## 6. Verification commands

On the current CLI:

```sh
marqdo check entry.mq.md --emit analysis.json
python3 tests/consumers/validate_protocols.py analysis.json observation.json tool.json
python3 tests/consumers/analysis_consumer.py analysis.json
marqdo inspect entry.mq.md --refs --json
```

For a downstream checkout, pin the CLI archive and run its real fixtures
against `MARQDO_BIN`; record the CLI SHA and the Analysis schema hash in the
consumer's evidence. See the issue accompanying this document for the exact
downstream acceptance items.
