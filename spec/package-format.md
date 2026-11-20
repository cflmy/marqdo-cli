# Marqdo Project Package Format v2

Status: **lock_version 2 as implemented by M0.6 R1.** Path sources and
package-instance identity are implemented; pinned Git sources, registries,
transitive dependency solving and environment materialization are added in R5
or later; see §8.

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
* for every plugin requirement, `version` is non-empty.

Note: a *git* dependency passes manifest validation when pinned, but the
dependency is not resolvable or lockable in this build (§5).

### 2.3 `capabilities` are requests

The manifest is the project's **request** for permissions. It is never read
when constructing the host capability set; only CLI flags/grant APIs create
grants (`spec/security.md` §2). `net` has no implementing host op, and
`process`/foreign execution still needs `--allow-foreign`.

## 3. `marqdo.lock`

`lock_version: 2`, packages sorted by name:

```yaml
lock_version: 2
packages:
- name: geo
  instance_id: dep:geo@0.1.0#101a6f6db4cc
  source: path:./packages/geo
  sha256: 101a6f6db4cc48d8c138b4716028da9bc8934b5f02b2235e020fbcabba15b757
```

| Field | Semantics |
|---|---|
| `lock_version` | writers emit `2`; readers accept `1` and `2` and migrate v1 entries by deriving an instance id |
| `packages[].name` | dependency name from the manifest |
| `packages[].instance_id` | logical `dep:<name>@<version>#<digest-prefix>` identity (ADR-0008) |
| `packages[].source` | `path:<manifest path as written>`; a Git source is `git:<url>@<rev>` |
| `packages[].sha256` | optional; lowercase hex SHA-256 of the dependency tree |

### 3.1 Deterministic serialization

`LockFile::to_yaml` sorts packages by name before serializing. The format
contains no timestamps, no absolute paths and no machine-specific values, so
repeated `marqdo lock` runs over unchanged dependencies are byte-identical.
`LockFile::from_yaml` also sorts packages on load, so equality comparison is
order-insensitive.

### 3.2 Tree hash

For a path or Git dependency, `lock` hashes the resolved package tree
(`marqdo_package::hash_tree`):

1. walk the dependency directory with `walkdir` (`sort_by_file_name`), keeping
   regular files only (symlinks are not followed and are skipped);
2. sort files by their root-relative path (normalized to `/`);
3. feed into SHA-256, for each file in order:
   `relative_path bytes`, a `0x00` byte, `file bytes`, a `0x00` byte;
4. hex-encode the digest.

This is a deterministic content fingerprint, not a Merkle tree and not a
cryptographic commitment to file modes, symlinks or empty directories. It
detects content changes and additions/removals of regular files.

### 3.3 Lock generation rules

`marqdo lock` (run in a project):

1. discovers the project root from the cwd;
2. loads and validates `marqdo.yaml`;
3. for each manifest dependency, in map (sorted) order:
   * a `path` source: the directory/file must exist; its tree is hashed;
   * a `git` source: an exact `rev` is required. `lock` clones it into
     `.marqdo/cache/git/<url-digest>-<rev>`, checks out that revision, strips
     `.git`, and hashes the tree. Install hooks and submodules are not run;
   * no source → error;
4. records `instance_id`, `source` and `sha256` for each package;
5. writes `marqdo.lock` (exit 0, `locked N package(s)`).

What it does **not** do: resolve SemVer ranges, download a registry, verify
package signatures, install packages or run dependency scripts. A dependency's
own `marqdo.yaml` is used for transitive `@alias` imports (R1/R5); the lock is
still a flat direct-dependency record.

## 4. `sync`

`marqdo sync [--locked]` (run in a project):

1. discovers the root; loads the manifest and the existing `marqdo.lock`
   (missing lock → error "run `marqdo lock` first", exit 1);
2. recomputes the expected lock file exactly as `lock` would (including Git
   cache resolution and tree hashes);
3. with `--locked`, compares the expected lock with the on-disk lock; a
   difference (manifest change, source change or dependency content change)
   prints "lockfile is out of date or dependency contents changed; run
   `marqdo lock`" and exits 1;
4. materializes the locked packages into a project-private
   `.marqdo/env/<instance-id>/` tree, writes `.marqdo/env/env.json`, and
   publishes it atomically (staging + rename, old env restored on failure);
5. without `--locked`, also refreshes the env; with `--locked`, the materialized
   content must match the lock. `run`/`check` prefer a materialized copy only
   when its tree digest matches the resolved package digest.

`sync` never rewrites the lock file. There is no `--offline` flag yet; a
missing Git cache causes a clear "not cached; run sync/lock" error rather than
an implicit network fetch during `check`/`run`.

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
* writes `index.mq.md`:

  ```markdown
  ## main

  **问候 = "你好，Marqdo"**

  > 打印(问候)

  *问候*
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
| Transitive dependency graph and conflict resolution | Transitive `@alias` imports resolve from the dependency's own manifest; the lock remains flat and a full conflict solver is not implemented. |
| `tree`, `update`, `vendor`, `package` commands | Not implemented. |
| `--offline`, `--locked` on `lock` | Not implemented; missing cache fails instead of fetching during check/run. |
| Environment materialization (`.marqdo/env`, `cache`) | Implemented: `sync` writes `.marqdo/env` atomically and `lock` populates the Git cache. Kernels/sessions are not materialized. |
| Dependency signatures / trusted publishers / supply-chain scanning | Not implemented. |
| Multiple/nested project boundary diagnostics | Not implemented; nearest `marqdo.yaml` wins. |
| Lock file migration between `lock_version`s | v1 is read and migrated; writers emit v2. |
| Plugin installation / plugin dependency management | Not implemented; `plugins:` in the manifest is declarative only. |

The lock/package format is deliberately small and deterministic so these can
be added later without changing the identity model. Changing `lock_version`,
`source` syntax or the tree-hash algorithm requires an ADR and a migration
plan.
