# Marqdo extension selection and lock v4

Status: **implemented** (RFC-EXT-0001 §6, ADR-0056). This is the normative
contract for `marqdo.yaml.extensions`, `marqdo.lock` extension identities, and
how `lock`/`sync`/`check`/`run`/`inspect` treat them.

## 1. Scope

An **extension** is one of:

| Kind | Manifest/descriptor | Execution form |
|---|---|---|
| `plugin` | `plugin.yaml` (Plugin ABI v1) | native/worker callable plugin |
| `kernel` | `kernel.json` (Foreign wire v1) | one-shot child process |
| `provider` | `provider.json` (Resource Provider v2) | worker helper + C ABI library |

Extension *selection* is distinct from extension *authorization*: selecting a
manifest makes its names visible and allows `run` to consider it. It never
creates a grant, loads a library, starts a worker or executes a script. Actual
permissions are the CLI grants plus the capability broker at the effect site.

## 2. Project manifest

```yaml
extensions:
  plugins:
    - from: ./adapter/plugin.yaml
  kernels:
    - from: "@quantum/adapter/kernel.json"
  providers:
    - from: "@quantum/adapter/provider.json"
```

Rules:

1. `from` is exactly one of:
   * `./relative/path` — below the project root;
   * `@alias/relative/path` — below a dependency declared in
     `dependencies` in the same project.
2. Absolute/rooted paths, `http(s)://` URLs, globs, empty strings, `.`-only
   paths and any `ParentDir` (`..`) component are rejected. The alias must
   exist in `dependencies`.
3. The resolved path must be a regular file whose canonical form stays inside
   its owning package root (project root or dependency package root). Symlink
   escapes are rejected.
4. Duplicate `(kind, from)` entries are rejected.
5. A dependency's extension file is identified by the dependency package
   instance id (`dep:name@version#content-digest`), never by a physical cache
   path.
6. Unknown YAML fields are ignored by the parser, but they never imply
   execution or authorization.

The existing `plugins:` block is a **version requirement** and is not a
runnable artifact; it remains declarative only.

## 3. Lock v4

```yaml
lock_version: 4
content_profile_version: 1
root:
  package: root:example@0.1.0
  manifest_digest: <64 hex>
packages: []
root_dependencies: {}
extensions:
  - kind: plugin
    logical_owner: root:example@0.1.0
    name: demo.solver
    version: 0.1.0
    manifest_digest: <64 hex>
    artifact_digest: <64 hex>
    selected_target: x86_64-unknown-linux-gnu
    source: ./adapter/plugin.yaml
```

| Field | Rule |
|---|---|
| `kind` | `plugin` \| `kernel` \| `provider`; unknown values are rejected |
| `logical_owner` | non-empty package instance id, root or dependency |
| `name` | stable declared name; for a kernel, `provider_id` |
| `version` | declared SemVer/string; a kernel has no version field, so `wire:<n>` |
| `manifest_digest` | SHA-256 of the exact manifest/descriptor bytes (lowercase hex) |
| `artifact_digest` | declared selected-target artifact SHA-256; for a kernel the pinned provider executable digest |
| `selected_target` | host target triple selected at lock time |
| `source` | `from` string exactly as declared |

* Extensions are sorted by `(kind, name, logical_owner)`; the whole array is
  omitted when empty.
* Duplicate `(kind, name, logical_owner)` or duplicate `(kind, source)` is an
  error.
* `manifest_digest`/`artifact_digest` are integrity values. They are not
  signatures and do not establish publisher identity.
* Lock v3 is rejected with an explicit migration message; run `marqdo lock`.
  No v3 field is reinterpreted as v4 and the lock is not rewritten implicitly.

## 4. Stage behaviour

### 4.1 `marqdo lock`

1. Resolves the package graph as before.
2. Resolves every `extensions` source to a canonical regular file.
3. Reads each manifest/descriptor statically and computes the identity above.
4. Rejects missing files, malformed manifests, unsupported provider/kernel
   schema versions, missing current-target artifacts and duplicate identities.
5. Writes lock v4 atomically. It never loads a shared library, starts a helper
   or executes a kernel.

### 4.2 `marqdo sync [--locked]`

1. Loads and validates the lock (v3 → explicit migration error).
2. Resolves the current declared sources and computes their identities.
3. Compares the declared selection with the lock (same rules as §4.3).
4. `--locked` forbids fetching; any lock/graph mismatch is fail-closed.

`sync` materializes package content but does not authorize or execute an
extension binary.

### 4.3 `marqdo check`

`check`:

* loads sources, static manifests and the lock;
* compares declared extension identity with the lock and fails with
  `ext.extension_lock_mismatch` on a missing/extra/changed identity;
* may read manifests and declared digests, but **never** loads a plugin
  library, starts a provider worker, spawns a foreign kernel or runs a
  side-effect script;
* does not require a plugin/provider/kernel artifact file to exist for the
  static check path (execution-artifact verification belongs to `run`).

### 4.4 `marqdo run` / `marqdo call`

Before the entry function runs:

1. `prepare` performs the same static binding and lock comparison as `check`.
2. The merged selection (project declaration + explicit CLI flags) determines
   which plugin/provider/kernel descriptors are considered.
3. Real artifact bytes/hashes, bundle containment, target and exact grants are
   verified before any worker/library/kernel is started.
4. A failed verification produces a structured `ext.*` error and the main
   function does not run. There is no fallback to "latest", PATH discovery or
   a trusted in-process native provider.

`--mode pure` and `--mode strict` reject project-declared extension activation
(`cli.invalid_grants` / `strict.unsupported_grant`) before any extension is
used.

## 5. Selection priority and conflicts

1. Project declaration and CLI selection are merged and deduplicated by
   canonical path.
2. Identical logical identity + digests is idempotent.
3. Same package/kernel/provider logical identity with different manifest or
   artifact digest is `ext.duplicate_callable` (prepare) — never last-wins and
   never auto-suffixed.
4. Two providers declaring the same resource kind is
   `ext.duplicate_provider_kind` (before spawn).
5. Ordering of CLI flags and YAML entries never decides which implementation is
   used.

## 6. `inspect --extensions`

`inspect FILE --extensions [--json] [--plugin PATH]` prints:

* declared sources with `kind`, `source`, package-relative `path`, logical
  owner and resolution status (`locked`/`unlocked`);
* the locked identity fields when available;
* static plugin descriptors (package/version/kind/target/artifact digest/
  manifest digest/exports), validation `static_manifest_ok`;
* official stdlib public aliases (`stdlib_aliases`) with the verified target.

Authorization is reported as `not_granted`; inspection never grants, loads or
executes anything. Environment variables, secrets and binary contents are never
printed.

## 7. Diagnostics

| Code | Stage | Trigger |
|---|---|---|
| `ext.extension_lock_mismatch` | prepare/sync | declared selection missing from lock, extra locked extension, changed manifest/artifact identity, or extensions declared without a lock |
| `ext.duplicate_callable` | prepare | two different manifests for one package identity, or duplicate callable names |
| `ext.duplicate_provider_kind` | prepare/run init | two providers declare the same resource kind |
| `ext.no_matching_artifact` | identity/manifest read | no artifact for the current target |
| `ext.artifact_integrity` | run init | artifact bytes/hash/containment mismatch |
| `project.lock` | load | lock v3/older version, malformed lock |
| `project.manifest` | load | invalid `extensions` source form, missing file, alias not declared |

Exit codes follow the CLI convention: manifest/option errors are `2`, load/
verify/runtime errors are `1`; `check` exit code is `1` on any compile/load
error, `0` on success.
