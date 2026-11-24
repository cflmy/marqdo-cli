# Protocol convergence policy (pre-stable)

Status: **current contributor policy**. Owner: architecture review. Authority:
[ADR-0068](../adr/0068-single-current-public-contracts.md), adopted for
[issue #8](https://git.cflmy.cn/cflmy/marqdo-cli/issues/8) before the first
stable release. This page is the checklist that prevents the same failure from
returning; it does not replace `spec/` or an ADR.

## 1. The failure this policy prevents

Before issue #8 the tree kept several competing models of the same fact:

- Analysis chose `schema_version` 2 or 3 from the **content** of the project
  (presence of `break`/`continue`), and a Tool declared an **array** of
  Analysis versions and installed when the arrays intersected. Installation
  succeeded while the actual output stayed unreadable for that project.
- Trace and Execution Events each defined a public representation of the same
  execution, with separate writers, budgets and privacy rules.
- `spec/` still carried superseded Analysis/Trace/Events documents beside the
  current ones, so a reader could not tell which contract the binary emitted.

The root cause was not "too many JSON files". It was **maintaining a replaced
architecture as a second live path** after the architecture had changed. The
cure is one current contract per real boundary, replaced in place, with
retired inputs rejected explicitly.

## 2. Current boundary map

Every row has exactly one current contract. A change must keep it that way.

| Real boundary | Current contract | Single producer / validator | Retired, kept in Git only |
|---|---|---|---|
| Markdown syntax | [`markdown-profile.md`](../../spec/markdown-profile.md) `marqdo.md.v1` | `marqdo-markdown` (frozen Comrak options) | older profiles |
| Language mapping | [`mapping.md`](../../spec/mapping.md), [`expression.ebnf`](../../spec/expression.ebnf) | `marqdo-frontend` + `marqdo-expression` | pre-RFC-LANG-0002 mapping |
| Static facts | [`analysis.md`](../../spec/analysis.md) + `analysis.schema.json` | `crates/cli/src/analysis.rs` | Analysis v1/v2/v3 split |
| Runtime facts | [`observation.md`](../../spec/observation.md) + two schemas (event, snapshot) | Host observation sink, CLI assigns `run_id`/`seq` | Trace v1/v2, Execution Events v1 |
| Installed commands | [`cli-tool.md`](../../spec/cli-tool.md) + `cli-tool.schema.json` | `crates/cli/src/tool.rs` | Tool v1 arrays/negotiation |
| C ABI | [`plugin-abi.md`](../../spec/plugin-abi.md) + C header | `marqdo-plugin-abi`, SDK | obsolete ABI major |
| Foreign kernels | [`foreign.md`](../../spec/foreign.md) wire v1 | `marqdo-foreign` (one-shot) | retired wire/versions |
| Resource providers | [`resource-provider-v2.md`](../../spec/resource-provider-v2.md) manifest v2 | CLI manifest validation + Host worker | retired manifests |
| Worker plumbing | [`worker-ipc.md`](../../spec/worker-ipc.md) (internal, no version) | `marqdo-worker-ipc` | private framing copies |
| Extension selection + lock | [`extension-selection.md`](../../spec/extension-selection.md), lock v4 | `marqdo-package` + CLI | lock v1–v3 readers/migrators |
| Values | [`value-wire.md`](../../spec/value-wire.md) | `marqdo-value::wire` | alternate encoders |

A boundary may have **two root objects** of one contract (currently Observation
event and snapshot). Two root objects are not two protocols: they share one
event definition and one schema authority.

## 3. Rules

### 3.1 One current contract per boundary

- One current format identifier, one producer, one validation path.
- No selection of a schema from project content, feature detection or runtime
  state.
- No multi-version arrays, version ranges, "highest shared version", capability
  probing or intersection negotiation.
- No simultaneous emission of an old and a new format, and no compatibility
  adapter between two writers.

### 3.2 Replace in place, in one integration

- Change the producer, specification, JSON Schema, independent consumers,
  fixtures and CI in the same change series. The intermediate commits on a
  branch may be incomplete; a merged state may not contain two live paths.
- Delete the replaced implementation, spec and fixtures in that integration.
  Git history and tagged archives are the archaeology; `spec/` is not.
- Keep the existing format identifier when the contract **shape** does not
  change. Do not renumber formats for appearance, symmetry or "freshness"
  (**no version bump without a shape change**). Conversely, when the shape
  changes, an in-place replacement at the existing identifier is allowed
  pre-stable and preferred over adding a parallel version.

### 3.3 Reject, never migrate silently

- Unsupported formats, versions, fields and command options fail with a
  corrective action (for example "run `marqdo lock` to regenerate", "remove and
  reinstall the tool").
- No automatic upgrade of an old lock, tool record, manifest or machine output;
  no field guessing; no fallback to a legacy reader; no "temporarily accept
  both".
- Keep historical formats retrievable from Git and release tags, but do not let
  the current binary read them.

### 3.4 Never trade a safety invariant for convergence

Removing a compatibility path must not remove integrity, authorization, type
checks, worker timeouts, size limits, secret handling or OS-isolation gates.
Convergence is about the fact model, not about relaxing it. When deleting code
that looks like "legacy compatibility", confirm first that it is not one of:

- Analysis/Observation digest binding and source attribution;
- tool bundle hashing, entrypoint/path/symlink checks, command-conflict checks;
- plugin/provider artifact target/path/sha256 verification and native trust;
- capability grants, declared-vs-installed handler checks, budgets;
- strict mode fail-closed behavior and worker quarantine;
- Value Wire type fidelity and non-transferable handle rules.

### 3.5 Preserve privacy when merging fact models

A unified model must keep the strictest privacy default of the models it
replaces. Today value observation is off by default; selected names must be
declared; Text needs separate consent; Memory/Handle are never serialized.
Merging Trace into Observation never blesses Trace-era value exposure.

## 4. Mandatory change procedure

For any change to a public boundary, in order:

1. **ADR.** Record the single target design, the boundary, the identifier
   decision (keep or replace) and what is deleted. If an ADR is superseded, mark
   its header Historical so no reader treats it as current.
2. **Specification in place.** Update the one current `spec/*.md` (never add a
   parallel "v-next" current spec). Update the JSON Schema if one exists.
   `spec/` must describe only what the current tree emits.
3. **One producer, one validator.** Delete the alternate selection logic, the
   second writer, the legacy reader and their dead error paths in the same
   series. `grep` is evidence: no `legacy`, `compat`, `schema_v2_or_v3`,
   `shared_version`, version arrays or fallback branches may remain runnable.
4. **Fixtures and consumers.** Replace goldens with ones produced by the **real
   CLI**, update the independent consumer(s), and add negative fixtures for the
   retired input. Never weaken or delete a security/semantic negative test to
   go green.
5. **CLI and docs.** Update `--help`, README, developer guides, examples and
   release/smoke scripts. No alias for a retired option.
6. **Enforcement gate.** Add or keep a source-level check that the duplicate
   cannot quietly return (see `architecture_boundaries` tests; for example the
   shared worker transport test forbids reimplementing framing).
7. **Evidence.** Record the executed commands in `docs/STATUS.md` and an
   evidence file with the exact SHA. A design document is not evidence.

## 5. Anti-patterns — do not do these

| Tempting move | Why it is rejected | Do instead |
|---|---|---|
| Keep v2 output for "simple" projects | Content-driven schema selection is the original bug | One writer, one identifier |
| Accept `analysis: [2,3]` and pick the intersection | Installs succeed when output is unreadable | One exact requirement, reject mismatch |
| Add `--legacy`, `compat`, `map_v1` or per-file switch | Reintroduces a second live architecture | Replace the current path; delete the old one |
| Keep the old writer behind a new "Unified" trait | Cosmetic abstraction over two fact models | One producer feeding all destinations |
| Rename `analysis-v3.md` to `analysis.md` only | Docs claim what code does not do | Update code, schema, fixtures, consumers together |
| Accept old lock/tool/extension silently | Hides a broken upgrade and loses integrity | Fail with an explicit regenerate/reinstall message |
| Delete security tests to make CI green | Convergence must not lower the bar | Keep/strengthen negatives |
| Renumber an unchanged format "to be safe" | Creates migration cost without a shape change | Keep the existing identifier |
| Add a second observation writer per destination | Two sources of truth for one execution | One event sequence, many destinations |
| Share worker capabilities with shared framing | Transport sharing is not authority sharing | Share framing only; keep grants per boundary |

## 6. Review checklist

A reviewer must be able to answer "yes" to all of these before approving a
protocol-boundary change:

- [ ] Does this boundary still have exactly one current contract and one
      producer/validator?
- [ ] Did the replaced spec/implementation/fixtures get deleted in this
      integration (not scheduled "later")?
- [ ] Are retired inputs rejected with a corrective action, with no migrator,
      alias, fallback or field guessing?
- [ ] Is the format identifier unchanged unless the contract shape changed?
- [ ] Do independent consumers and real-CLI fixtures validate the new artifact,
      including at least one negative case?
- [ ] Are digest binding, attribution, grants, budgets, timeouts, size limits,
      secret handling and strict-mode fail-closed behavior intact and tested?
- [ ] Does the privacy default stay structural, with explicit value/Text
      consent?
- [ ] Are `--help`, README, developer docs, examples and release smoke updated
      to the current contract only?
- [ ] Is there a source-level gate preventing the duplicate path from returning?
- [ ] Is the executed evidence recorded against a SHA (not a plan or a claim)?

## 7. Definition of Done for a convergence change

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace --locked` pass.
- `cargo test -p marqdo-cli --test architecture_boundaries --test
  docs_consistency` pass.
- Independent consumers validate real CLI output; schema files parse; retired
  options and formats are proven rejected.
- `spec/` contains no superseded current-contract document; retired ADRs carry a
  Historical header; `docs/STATUS.md` names the executed evidence.
- The release archive smoke test is updated before an RC is tagged, and the RC
  evidence is bound to the archive's SHA.

## 8. Stable-release boundary

Everything above applies **before the first stable release**. After the first
stable release a separate compatibility policy must define which contracts are
stabilized, the deprecation window and the upgrade rules. Do not import
post-stable compatibility obligations into the pre-stable tree, and do not
assume this policy authorizes breaking stable contracts later.

## 9. Where the policy is enforced

| Enforcement | Artifact |
|---|---|
| Absolute language/architecture rules | [`AGENTS.md`](../../AGENTS.md) |
| Public-contract decision | [ADR-0068](../adr/0068-single-current-public-contracts.md) |
| Architecture overview and boundary table | [`architecture/CURRENT.md`](CURRENT.md) |
| Normative contracts | `spec/*.md` + `spec/schemas/*.json` |
| Source-level boundary gates | `crates/cli/tests/architecture_boundaries.rs` |
| Docs/link/status gates | `crates/cli/tests/docs_consistency.rs` |
| Independent consumers | `tests/consumers/*.py` |
| Release evidence | [`STATUS.md`](../STATUS.md), `docs/evidence/`, `docs/releasing.md` |
