# ADR-0056: Explicit extension selection and lock v4

- Status: Accepted (2026-11-21)
- Implements: RFC-EXT-0001 §6 (PR-04)
- Related: ADR-0008/0016/0039, ADR-0054/0055/0057/0058

## Context

`marqdo-package` already locks a reproducible package-instance graph (path and
pinned Git sources) and materializes it under `.marqdo/env`. Extensions had no
equivalent: plugin/provider/kernel paths were only accepted as repeatable CLI
flags (`--plugin`, `--kernel`, `--provider`). An independent repository could
therefore publish a `.mq.md` package and a callable/extension artifact, but a
consumer could not record *which exact manifest/artifact* the project selected,
and a project manifest could not declare activation sources at all.

Copying CLI paths on every run makes deployments non-reproducible and invites a
second, hidden package manager. The existing lock is the right place, but its
v3 schema has no extension identity.

## Decision

### 1. `marqdo.yaml.extensions` declares activation sources, never grants

```yaml
extensions:
  plugins:
    - from: ./adapter/plugin.yaml
  kernels: []
  providers:
    - from: "@quantum/adapter/provider.json"
```

Only two `from` forms exist:

* `./relative/path` inside the project;
* `@alias/relative/path` inside a **declared** dependency in the same fixed
  package graph.

Absolute paths, URLs, globs, `..` traversal and implicit discovery are rejected
at manifest validation; the resolved file must be a regular file inside its
owning package. A declaration means "this project activates this extension" —
it never means permission: `--allow-plugin`/`--allow-provider`/`--allow-foreign`
and the capability broker remain separate. `pure`/`strict` refuse to activate
project-declared extensions.

CLI flags remain supported as an explicit temporary/dev selection. Selection is
deduplicated by canonical path; the same manifest selected from the project
and the CLI is idempotent, while two different manifests for one package
identity are a deterministic `ext.duplicate_callable` error.

### 2. Lock v4 records extension identity

`marqdo.lock` becomes `lock_version: 4`. The package-instance graph is
unchanged; a new `extensions:` array records, per selected source:

```yaml
- kind: plugin            # plugin | kernel | provider
  logical_owner: root:example@0.1.0
  name: demo.solver
  version: 0.1.0
  manifest_digest: <sha256 of exact manifest/descriptor bytes>
  artifact_digest: <selected target artifact sha256; kernel executable digest>
  selected_target: x86_64-unknown-linux-gnu
  source: ./adapter/plugin.yaml
```

The array is sorted by `(kind, name, logical_owner)`. Duplicate identities or
duplicate sources are rejected. `manifest_digest`/`artifact_digest` are
**integrity** values, not publisher authentication; a release signature/trust
root is separate work.

### 3. Explicit migration, no silent reinterpretation

`LockFile::from_yaml` of a v3 document returns a deterministic error: lock v3 is
superseded by v4 (extension identities), run `marqdo lock` to migrate. v1/v2
keep their relock error but now name v4. `run`/`check`/`sync` never rewrite the
lock and never fetch a floating revision.

### 4. Who verifies what

| Stage | Behaviour |
|---|---|
| `marqdo lock` | Resolves declared `from` sources statically, reads manifests and declared artifact digests, writes v4. No extension binary is loaded or executed. |
| `check` | Loads project + static descriptors and compares declared identities with the lock (`ext.extension_lock_mismatch` on missing/extra/changed). Never starts a worker; a missing artifact is not fatal for a pure static check. |
| `sync` | Re-validates declarations against the lock before materializing package content. |
| `run`/`call` | Re-verifies the actual artifact bytes/hashes for selected plugins/providers (and the kernel executable) before spawning/loading anything; exact grants are still required. |
| `inspect --extensions` | Projects declared sources, lock identity, static plugin descriptors and official stdlib aliases; authorization is always `not_granted` and inspection never executes. |

### 5. Selection priority and conflicts

1. Project declaration (locked) and explicit CLI selection are merged by
   canonical path; identical identity is idempotent.
2. Same package/kernel/provider logical identity with different manifest digest
   or artifact digest is an error; no `last wins`, no auto-suffix.
3. Extension logical owners are recorded as package instance ids (root or
   dependency), so moving the cache/checkout does not change identity.
4. A dependency extension cannot escape its package root (`@alias/../` is
   rejected; canonical path must stay under the package root).
5. Multi-version dependencies remain distinguishable by package instance +
   alias; the lock records the exact owner.

## Consequences

* Third-party `.mq.md`/plugin/kernel/provider repositories can publish into a
  project with a reproducible, auditable selection;
* `check` stays side-effect free while still validating extension identity;
* pre-existing projects migrate with one `marqdo lock`; their v3 lock is not
  silently accepted;
* the existing package graph, Git pinning, strict `sync` and env materialization
  are unchanged apart from the version bump and new section;
* no package registry, network discovery or automatic extension installation is
  introduced.

## Rejected alternatives

* **Keep extensions CLI-only.** Rejected: projects cannot reproduce/audit the
  selection, and `check` cannot validate it.
* **Put extension paths in the existing `plugins:` requirement map.** Rejected:
  it is a version *requirement*, not a runnable artifact identity, and mixing
  them would make manifests ambiguous.
* **Add fields to lock v3 without bumping the version.** Rejected by
  RFC-EXT-0001 §6.3 and the repository's public-lock rule.
* **Let extensions be discovered from directories, PATH or an HTTP index.**
  Rejected: implicit discovery is non-deterministic and un-auditable.
* **Auto-grant from a manifest declaration.** Rejected: selection is not
  authorization (ADR-0054/0055).