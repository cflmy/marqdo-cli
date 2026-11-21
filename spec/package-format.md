# Marqdo Project Package Format v4

Status: **lock_version 4 package-instance graph plus extension identities
(RFC-EXT-0001 §6, ADR-0056).** Path and pinned Git sources, transitive closure
locking, fail-closed locked resolution, strict `sync`, content-profile-1
file-set hashing/materialization, and explicit `extensions:` activation
sources/identities are implemented. Registries and SemVer solving remain out of
scope; see §8. Lock v3 is rejected with an explicit `marqdo lock` migration
error; it is never reinterpreted silently.

Implementation: `crates/marqdo-package/src/lib.rs` (manifest/lock types and
validation), `crates/cli/src/commands.rs` (`init`, `add`, `remove`, `lock`,
`sync`).

## 1. Project root

The project root is the nearest ancestor directory of the entry document that
contains `marqdo.yaml` (`marqdo_package::discover_root`). If no manifest exists
anywhere above the entry, the entry file's parent directory is used as the
root (a document can be checked/run without a project).

`ModuleId`s are package-instance + package-relative paths (`spec/references.md`
§2.1), so the project root selects the entry package instance; physical paths do
not form module identity. `add`/`remove`/`lock`/`sync` discover the
root from the **current working directory**, not from a document path. There is
no `--project` flag in this build.

## 2. `marqdo.yaml`

```yaml
project:
  name: example
  version: 0.1.0
  marqdo: ">=0.1,<1"          # optional; informational in this build
dependencies:
  geometry:
    path: ./packages/geometry # exactly one of path / git+rev
  quantum:
    git: https://github.com/example/quantum-mq
    rev: 0123456789abcdef0123456789abcdef01234567
plugins:
  solver:
    version: "^1.0"            # requirement only; not installed or authorized
capabilities:                  # requests only; not grants
  fs:
    read: ["./data/**"]
    write: ["./output/**"]
  net:
    hosts: []
  process: false
extensions:                    # explicit activation sources; never grants
  plugins:
    - from: ./adapter/plugin.yaml
  kernels: []
  providers:
    - from: "@quantum/adapter/provider.json"
```

### 2.1 Fields

| Field | Type | Required | Semantics |
|---|---|---|---|
| `project.name` | string | yes | non-empty package name |
| `project.version` | string | yes | non-empty version string (not semver-validated in the manifest) |
| `project.marqdo` | string | no | required Marqdo runtime range; stored, not enforced by this build |
| `dependencies.<name>.path` | path string | one of path/git | project-relative path to a dependency directory or `.mq.md` file |
| `dependencies.<name>.git` | URL string | one of path/git | git source (must be pinned with `rev`) |
| `dependencies.<name>.rev` | string | with `git` | exact revision; a bare `git` without `rev` is rejected |
| `plugins.<name>.version` | string | per entry | non-empty plugin version requirement; declarative only |
| `capabilities.fs.read` / `.write` | list of strings | no | requested project-relative globs |
| `capabilities.net.hosts` | list of strings | no | requested hosts |
| `capabilities.process` | bool | no | requested process execution |
| `extensions.plugins[]` / `.kernels[]` / `.providers[]` | `{from}` | no | explicit extension activation sources; `from` is `./relative/path` in the project or `@alias/relative/path` in a declared dependency. The alias must be in `dependencies`. Absolute paths, URLs, globs and `..` traversal are rejected. A declaration is not a grant. |

Unknown fields are ignored (the types do not `deny_unknown_fields`). Duplicate
YAML keys follow `serde_yaml` behavior (last one wins); there is no duplicate-key
diagnostic.

### 2.2 Validation (`Manifest::validate`)

`marqdo.yaml` is loaded with `Manifest::from_yaml`/`Manifest::load`, which
validates:

* `project.name` and `project.version` are non-empty after trimming;
* for every dependency, exactly one source is present:
  * both `path` and `git` → error "declares both path and git";
  * neither → error "must declare a path or a pinned git source";
  * `git` without `rev` → error requiring an exact revision;
  * an absolute `path` → error "path must be project-relative";
* for every plugin requirement, `version` is non-empty;
* every `extensions.*[].from` is one of the two allowed forms and resolves to a
  regular file inside its owning package; duplicates within a kind are
  rejected; `@alias` must name a declared dependency. Resolution happens when
  the project is loaded (`PackageResolver::resolve_extension_sources`), not in
  plain `Manifest::validate`.

Note: a *git* dependency passes manifest validation when pinned, but the
dependency is not resolvable or lockable in this build (§5).

### 2.3 `capabilities` are requests

The manifest is the project's **request** for permissions. It is never read
when constructing the host capability set; only CLI flags/grant APIs create
grants (`spec/security.md` §2). `net` has no implementing host op, and
`process`/foreign execution still needs `--allow-foreign`.

## 3. `marqdo.lock`

`lock_version: 4` records the complete package-instance closure reachable from
the root manifest plus the exact identity of every declared extension.
Packages are sorted by `instance_id`; extensions by `(kind, name,
logical_owner)`:

```yaml
lock_version: 4
content_profile_version: 1
root:
  package: root:quantum@0.1.0
  manifest_digest: 4f5c...
packages:
- instance_id: dep:math@1.0.0#9e2...
  name: math
  version: 1.0.0
  source:
    kind: path
    path: deps/math
  content_digest: 9e2...
  manifest_digest: 7a1...
  dependencies:
    helper: dep:helper@2.0.0#1b3...
- instance_id: dep:helper@2.0.0#1b3...
  name: helper
  version: 2.0.0
  source:
    kind: git
    url: https://example.invalid/helper.git
    rev: 0123456789abcdef0123456789abcdef01234567
  content_digest: 1b3...
  manifest_digest: 3c9...
  dependencies: {}
root_dependencies:
  math: dep:math@1.0.0#9e2...
extensions:
- kind: plugin
  logical_owner: root:quantum@0.1.0
  name: demo.solver
  version: 0.1.0
  manifest_digest: 4f5c...            # sha256 of the exact plugin.yaml bytes
  artifact_digest: 9e2...             # selected target artifact sha256
  selected_target: x86_64-unknown-linux-gnu
  source: ./adapter/plugin.yaml
```

An extension lock record binds:

* `kind` — `plugin` | `kernel` | `provider`;
* `logical_owner` — root instance or dependency package instance id;
* `name` / `version` — stable declared identity (kernel descriptors have no
  version, so the lock records `wire:<n>`);
* `manifest_digest` — SHA-256 of the exact manifest/descriptor bytes;
* `artifact_digest` — declared selected-target artifact SHA-256 (for a kernel,
  the pinned provider executable digest);
* `selected_target` — the host triple selected at lock time;
* `source` — the `from` string exactly as declared.

`marqdo lock` records these identities statically (no extension is executed).
`check`/`sync` compare the current declaration to the lock and fail with
`ext.extension_lock_mismatch` on a missing, extra or changed identity; `run`
additionally verifies the real artifact bytes before starting any worker.
`extensions` is omitted when no extension is declared.

### 3.1 Lock v3 migration

`from_yaml` of a v3 document returns a deterministic error: lock v3 is
superseded by v4 (extension identities), and the user must run `marqdo lock`
to migrate explicitly. No v3 field is silently reinterpreted. Lock v1/v2 keep
their existing relock error with the v4 requirement.

| Field | Semantics |
|---|---|
| `lock_version` | writers emit `3`; readers accept only `3` and tell v1/v2 users to run `marqdo lock` |
| `content_profile_version` | file-set/digest profile; only `1` is accepted |
| `root.package` | logical root instance `root:<name>@<version>` |
| `root.manifest_digest` | lowercase hex SHA-256 of the canonical `marqdo.yaml` |
| `packages[].instance_id` | `dep:<name>@<version>#<full content digest>` |
| `packages[].source` | `{kind: path, path}` (project-relative) or `{kind: git, url, rev}` (full commit hash) |
| `packages[].content_digest` | 64-character lowercase hex SHA-256 of the package file set |
| `packages[].manifest_digest` | optional lowercase hex digest of the dependency's own manifest |
| `packages[].dependencies` | this instance's alias → target instance edges |
| `root_dependencies` | root manifest alias → target instance edges |

The graph is the unit of reproducibility: a transitive dependency that can be
imported is also locked. Edges are keyed by `(parent instance, alias)`, so the
same alias in two packages may resolve to different instances.

### 3.1 Serialization and validation

`lock` writes atomically (temporary file, `sync_all`, rename); a failed
resolution leaves the previous lock untouched. `LockFile::to_yaml` sorts
packages by `instance_id`. The format contains no timestamps, absolute paths or
machine-specific values, so repeated `lock` runs over unchanged inputs are
byte-identical.

`LockFile::from_yaml` validates before returning:

* `lock_version == 3`, `content_profile_version == 1`;
* unique ids, each consistent with `dep:<name>@<version>#<content_digest>`;
* every digest is 64 lowercase hex characters;
* every source is normalized (path relative to the project root without `..`,
  Git revision a full lowercase 40/64-character hash);
* every edge target exists and every package is reachable from
  `root_dependencies`; empty aliases and self-edges are errors.

v1/v2 locks are rejected, not silently upgraded: a flat lock cannot represent
the closure and treating it as complete would re-open fail-open loading.

### 3.2 Tree hash

For a path or Git dependency, `lock` hashes the resolved package tree with
`content_profile_version: 1` (`crates/marqdo-package/src/fileset.rs`, ADR-0016):

1. walk the dependency directory with `walkdir` (`sort_by_file_name`,
   `follow_links(false)`), propagating every walk error;
2. exclude any path component `.git` or `.marqdo`; include `target/`;
3. accept regular files only. Symlinks and special files are hard errors, not
   skipped or followed; paths must be UTF-8, `/`-separated, relative and
   duplicate-free;
4. sort files by root-relative path (UTF-8 bytes); enforce budgets of 10 000
   files, 64 MiB per file, 512 MiB total and depth 64;
5. hash with domain separation and length prefixes:
   `b"marqdo.package.file-set\0"`, `profile_version: u32le`,
   `entry_count: u64le`, then for each file
   `rel_len: u64le`, `rel_bytes`, `content_len: u64le`, `content_bytes`;
6. hex-encode the digest.

Materialization uses the same selection and reads each file once, hashing the
exact bytes it writes; the published environment is accepted only when the
final digest equals the lock. This is a deterministic content fingerprint, not
a Merkle tree and not a commitment to file modes or empty directories. Modes
are normalized on materialization (`0644` files, `0755` directories on Unix).

### 3.3 Lock generation rules

`marqdo lock` (run in a project):

1. discovers the project root from the cwd;
2. loads and validates `marqdo.yaml`;
3. traverses the full dependency closure in sorted manifest order:
   * a `path` source must resolve inside the project root; its module root is
     hashed and recorded as a project-relative path;
   * a `git` source requires a full lowercase commit hash. `lock` clones into
     `.marqdo/cache/git/<url-digest>-<rev>`, checks out the revision, verifies
     `git rev-parse HEAD` equals it, strips `.git`, hashes the tree, and writes
     a sibling `.meta.json` marker (`url`, `rev`, profile, digest). Install
     hooks and submodules are not run;
   * package cycles, duplicate ids with conflicting content, depth >128 and
     more than 4096 instances are errors;
4. records every package instance with its alias→instance edges, plus the root
   manifest digest and root edges;
5. validates the graph and atomically writes `marqdo.lock` (exit 0,
   `locked N package(s)`).

What it does **not** do: resolve SemVer ranges, download a registry, verify
publisher signatures, install packages or run dependency scripts. A dependency's
own `marqdo.yaml` supplies its dependency edges.

## 4. `sync`

`marqdo sync [--locked]` (run in a project):

1. discovers the root; loads and validates the existing `marqdo.lock`
   (missing or v1/v2 lock → exit 1 with a relock instruction);
2. verifies the lock belongs to the current root manifest
   (`root.package` and `root.manifest_digest`); drift fails closed;
3. materializes **every** package in the locked closure into a project-private
   `.marqdo/env/pkg-<sha256(instance-id)>/` tree using the profile-1 file set
   and the lock's content digest;
4. writes `.marqdo/env/env.json` schema 2 with the content profile, canonical
   lock digest, root edges and one record per package; publishes via staging +
   rename, restoring the previous environment on failure;
5. never rewrites the lock. Plain `sync` may fetch a pinned Git revision named
   by the lock; `--locked` forbids fetching. Both modes verify contents and
   fail with "lockfile is out of date or dependency contents changed; run
   `marqdo lock`" on drift.

`run`, `call`, `check` and `inspect` never fetch and never write the lock. With
declared dependencies they require a lock. They resolve each `(parent instance,
alias)` edge in the lock; missing entries, digest/source drift and root-manifest
drift are errors. A materialized env or Git cache directory is trusted only
after its file-set digest and cache marker verify.

## 5. `init`

`marqdo init [dir]`:

* refuses to overwrite an existing `marqdo.yaml` (exit 1);
* creates the directory if needed;
* derives the project name from the directory's final path component (falling
  back to `example`);
* writes `marqdo.yaml` from `Manifest::template`:

  ```yaml
  project:
    name: <name>
    version: 0.1.0
    marqdo: '>=0.1,<1'
  ```

  (empty dependencies/plugins/capabilities are omitted);
* writes `index.mq.md` (the target syntax; the template in a pre-RFC binary
  still emits the old spelling and is part of the pending migration):

  ```markdown
  ## @exec main

  `问候 = "你好，Marqdo"`

  `打印(问候)`

  `return 问候`
  ```

* writes `.gitignore` containing `.marqdo/`;
* prints the next-step hint `marqdo check index.mq.md && marqdo run index.mq.md`.

`init` does not create `marqdo.lock`, `.marqdo/`, a plugin directory or a git
repository.

## 6. `add` / `remove`

Both commands edit **only** `marqdo.yaml` (and, for `remove`, prune a matching
stale lock entry). They do not fetch, install, resolve or authorize anything.

* `marqdo add <name> --path <dir>` or
  `marqdo add <name> --git <url> --rev <sha>`:
  validate the dependency shape (same rules as §2.2), refuse an existing name
  (exit 1), insert it, write the manifest, and tell the user to run
  `marqdo lock`. A pinned Git dependency is lockable/resolvable in R5.
* `marqdo remove <name>`: remove the dependency from the manifest (exit 1 if
  absent), and drop a same-named entry from an existing lock file. It does not
  re-hash or re-lock the rest of the project.

This differs from the original task brief that listed `add`/`remove` as
"not supported yet": the CLI does implement them as manifest editors. They
are intentionally shallow and should not be confused with a dependency
resolver.

## 7. JSON output

`check`, `inspect`, `run`, `call`, `plugin inspect`, `add`, `remove`, `lock`
and `sync` accept `--json` and print a single JSON object on stdout. Relevant
package shapes:

* `lock`: `{"command":"lock","success":true,"packages":[...]}`
* `sync`: `{"command":"sync","success":true,"locked":<bool>,"packages":[...]}`
* `add`: `{"command":"add","success":true,"dependency":"<name>","manifest":"<path>"}`
* `remove`: `{"command":"remove","success":true,"dependency":"<name>"}`

Errors in these commands are printed as plain `error: ...` on stderr even when
`--json` is passed (the JSON success envelope is not emitted; this is an
inconsistency to fix).

## 8. Not supported in this build (explicit)

| Capability | Status |
|---|---|
| Git dependency resolution (pinned rev, clone/fetch/checkout) | Implemented for exact revisions; no floating branches, submodules or dependency scripts. |
| Registry / package publishing / SemVer resolution | Not implemented. |
| Transitive dependency graph and conflict resolution | The lock records the full transitive closure with per-instance edges; two versions of one package can coexist. A general SemVer conflict solver is not implemented. |
| `tree`, `update`, `vendor`, `package` commands | Not implemented. |
| `--offline`, `--locked` on `lock` | Not implemented; missing cache fails instead of fetching during check/run. |
| Environment materialization (`.marqdo/env`, `cache`) | Implemented: `sync` materializes the full locked closure into `.marqdo/env` atomically; `lock`/`sync` populate a verified Git cache. Kernels/sessions are not materialized. |
| Dependency signatures / trusted publishers / supply-chain scanning | Not implemented. |
| Multiple/nested project boundary diagnostics | Not implemented; nearest `marqdo.yaml` wins. |
| Lock file migration between `lock_version`s | Only v3 is read; v1/v2 are rejected with a relock instruction. `lock` regenerates the graph. |
| Plugin installation / plugin dependency management | Not implemented; `plugins:` in the manifest is declarative only. |

The lock/package format is deliberately small and deterministic so these can
be added later without changing the identity model. Changing `lock_version`,
`source` syntax or the tree-hash algorithm requires an ADR and a migration
plan.
