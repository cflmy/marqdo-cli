# ADR-0069 — One public Analysis boundary for independent static tools

Status: Accepted for implementation on the pre-stable 3.0 line (2026-11-25).

## Context

Issue [#10](https://git.cflmy.cn/cflmy/marqdo-cli/issues/10) audited the
public Analysis contract at `master@e6066e6`. The CLI already restricts
View/Lint-style consumers to installed Tools, but the emitted
`marqdo.analysis` document only exposed program IR. It omitted the Markdown
document AST, the exact source text, the entry/context snapshot, heading
scopes and anchors, complete section ranges, and reference resolution states.
Independent tools therefore had to re-parse Markdown, guess section ends or run
a second non-public analysis path. Two implementations of Markdown-link
resolution existed (`analysis.rs` and `commands.rs`), the Analysis `unresolved`
state could not distinguish "not loaded" from "missing", and `analysis.rs`
re-read `marqdo.lock` after preparation.

## Decision

`marqdo check ENTRY --emit PATH` remains the only public static-analysis
producer. Before the first stable release, replace `marqdo.analysis` revision 3
in place under ADR-0068. There is one shape, one producer and one validation
path; retired fields are not read.

The projection is built by one typed encoder from the same
`project_prepare::prepare` facts consumed by `check`, `inspect`, `run` and
`call`:

* `project.entry_module_id`, `manifest_digest`, the captured `lock_digest`,
  stdlib identity and the static extension identities that actually took part
  in preparation;
* each loaded module's exact UTF-8 `source` and its `source_digest`, plus the
  public Markdown AST (`document`) in the shape already described by
  `spec/markdown-profile.md`;
* one heading index: canonical path, parent, effective mode, mode origin and
  source unit, heading span, complete section span and the function created by
  an Exec H2;
* the anchor map, functions, explicit imports, resources, static call bindings
  and diagnostics;
* every Markdown link through one scoped `document_refs` resolver that
  distinguishes `resolved`, `not_loaded`, `missing`, `ambiguous`, `external`,
  `outside_scope` and `invalid`.

The resolver is a pure function over an explicit catalog. Ordinary Analysis
uses the loaded-module catalog and never opens a link target. `check --refs`
and `inspect --refs` build a separate, explicitly limited catalog for direct
link targets and append diagnostics; they do not change the ordinary Analysis
module set and never execute or import a target.

Identity strings stay the current presentation for names that do not contain
the structural separators. Unit paths are now also exposed as the structured
`path` array, and identity display escapes `/`, `#` and `%` inside a path
segment so `<module>#<seg>/<seg>` cannot collide with a segment containing
`/`. `Function.local_name` and `namespace` remain structured fields; consumers
must not split display ids.

The public source field is an explicit local artifact. It is written with
owner-only permissions where the platform supports them, is never copied into
Observation, is never printed by default, and its digestion is the exact
UTF-8 bytes (SHA-256). Observation continues to bind to the SHA-256 of the
exact bytes emitted by `check --emit` with the same static options.

## Security and verification

The boundary remains non-executing: no plugin shared library, worker, foreign
kernel, network request or implicit module load. `--emit` publishes atomically
through an owner-only exclusive temporary file and rename, refuses to overwrite
a loaded source, manifest, lock file, symlink to one, or a hard link to one,
and only removes a previous artifact at a caller-specified output path under
the same guard.

Production checks local typed invariants and the canonical encoder; an
independent Python consumer validates the emitted bytes against the published
JSON Schema and semantic invariants, and mutation fixtures prove that missing
required fields, stale same-revision artifacts, tampered digests/spans,
duplicate ids, wrong ownership and invalid reference states are rejected.
`docs/STATUS.md` records exactly what was executed on which SHA; the
pre-stable line makes no compatibility promise for consumers linked to the
retired shape.
