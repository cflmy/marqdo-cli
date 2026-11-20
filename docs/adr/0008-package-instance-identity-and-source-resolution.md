# ADR-0008: Package-aware module identity and source resolution

Status: **Accepted**
Date: 2026-11-20
Supersedes: the path-only `ModuleId` interpretation used by the M0.5 loader

## Context

M0.5 identifies a module by its project-root-relative path
(`crates/marqdo-ir/src/lib.rs::ModuleId`). The loader canonicalises every file
and rejects anything outside the project root. This works for one project but
cannot represent:

* two locked versions of the same dependency at once;
* a dependency installed in a shared, content-addressed cache rather than below
  the importing project;
* a dependency whose physical cache directory moves without changing meaning;
* stable function/knowledge identities for Analysis v1.

The M0.6 plan (`docs/NEXT_PHASE_IMPLEMENTATION.md` §1A E06/E07, R1) makes this
the first architectural correction because Analysis v1 must not freeze a
physical-path identity.

## Decision

### 1. Module identity is logical

`marqdo_ir::ModuleId` becomes:

```rust
pub struct PackageInstanceId(pub String);

pub struct ModuleId {
    pub instance: PackageInstanceId,
    pub relative_path: String,
}
```

The canonical display/encoding is `<instance>::<relative_path>` with `/`
separators. A string without `::` is accepted only as a convenience for tests
and standalone documents and is interpreted as `root:default`.

`PackageInstanceId` is one of:

* `root:<project-name>@<version>` for a project entry package;
* `root:default` when no `marqdo.yaml` exists;
* `dep:<package-name>@<version>#<content-digest-prefix>` for a dependency
  instance.

The dependency suffix is derived from the locked content digest, not from the
cache directory. Moving a cache or copying a dependency therefore does not
change identity, while two different locked versions (or two different contents)
do. A dependency without its own manifest uses version `0.0.0` and its manifest
alias as the package name.

Physical paths remain available for diagnostics and IO, but they are not part
of `ModuleId`, `FunctionId` or `UnitId`.

### 2. Package resolution is a package-layer concern

`marqdo-package` exposes:

```rust
pub enum PackageOrigin {
    Root,
    Path { declared: PathBuf },
    Git { url: String, rev: String },
}

pub struct ResolvedPackage {
    pub instance_id: PackageInstanceId,
    pub name: String,
    pub version: String,
    pub origin: PackageOrigin,
    pub revision: Option<String>,
    pub content_digest: Option<String>,
    pub module_root: PathBuf,
    pub manifest: Option<Manifest>,
}

pub struct PackageResolver { /* project root + manifest + lock */ }

impl PackageResolver {
    pub fn resolve_entry(&self, entry: &Path) -> Result<ResolvedPackage, String>;
    pub fn resolve_alias(&self, alias: &str) -> Result<ResolvedPackage, String>;
}
```

The loader consumes `ResolvedPackage` and never branches on `path`/`git`
itself. Source kinds are implemented by the package crate: path dependencies in
R1, pinned Git revisions in R5. Additional sources (registry mirrors, vendor
directories) must implement the same interface rather than adding loader
conditions.

### 3. Import forms remain explicit

A front-matter import is still either:

* `./relative/file.mq.md` inside the current package; or
* `@alias` naming a manifest dependency.

No implicit searching, no PATH-like resolution and no network access while
checking. Markdown links remain static references, not imports.

### 4. Lock format v2

`marqdo.lock` becomes `lock_version: 2`. Each package entry records:

```yaml
- name: geometry
  instance_id: "dep:geometry@0.1.0#0123456789ab"
  source: "path:./packages/geometry"
  sha256: "<content digest>"
```

Readers accept v1 by deriving the instance id from the source + `sha256` (or
`dep:<name>@0.0.0#unlocked` when no digest exists); writers always emit v2.
`sync --locked` compares instance ids and digests, so a source or content change
fails rather than silently selecting the newest package.

## Alternatives

1. Keep path-only `ModuleId` and add a separate package id only in Analysis.
   Rejected: the public protocol would still expose physical identity and the
   runtime would not distinguish two versions.
2. Put the cache path in the instance id. Rejected: cache relocation changes
   identity and defeats reproducible knowledge/proof artifacts.
3. Encode source kind (`path`/`git`) in `ModuleId`. Rejected: the same logical
   package may later be served from a lockfile/vendor cache; identity must not
   depend on how it was fetched.
4. Require a global registry immediately. Rejected: local path and pinned Git
   sources are the M0.6 scope; a registry is a later, optional source.

## Compatibility

This is a deliberate 0.x breaking change (M0.6). Existing lockfiles are read
through the v1 migration path; old `ModuleId` strings without an instance prefix
are accepted as `root:default`. CLI JSON that previously printed
`index.mq.md` now prints `root:default::index.mq.md` (or
`root:<project>@<version>::index.mq.md`). No released API promised the old form.

## Security

`PackageResolver` remains static: it reads manifests/lockfiles and hashes
content, but never executes package code or install scripts. The loader rejects
paths that do not resolve inside the resolved package's `module_root`. Git
resolution is limited to an exact revision and is performed without running
repository hooks (`GIT_TERMINAL_PROMPT=0`, no submodules/install scripts).

## Testing

* package tests for instance-id stability across cache/path moves, v1 lock
  migration and source mismatch;
* loader tests for two versions of the same dependency, path traversal
  rejection and canonical diagnostics;
* CLI integration tests that move a path dependency and assert unchanged
  `ModuleId`/analysis output.

## Migration & rollout

R1 implements path sources and the new `ModuleId`; R5 adds pinned Git and
environment materialisation. `docs/STATUS.md` and `spec/references.md` are
updated in the same change. Analysis v1 (R4) is frozen only after this ADR is
implemented and its tests pass.
