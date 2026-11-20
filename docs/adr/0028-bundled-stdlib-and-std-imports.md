# ADR-0028: Bundled standard library, `std:` imports and digest verification (CM-A10)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A10, CM-A07, P2/P3), ADR-0027
Specs: `spec/analysis-v2.md`, `spec/references.md`
Evidence: `docs/evidence/2026-11-20-bundled-stdlib-cm-a10.md`

## Context

CM-A07 Compat-A put a real `.mq.md` print chain under `examples/stdlib/`, but it
was reachable only by relative path from a demo in the same directory. A
standard library that ordinary projects can use offline needs:

* a fixed bundle shipped with the CLI;
* an explicit import spelling that cannot be shadowed by a project file;
* a content identity so a different bundle with the same version cannot
  masquerade;
* no network, Git or package installation required at run time.

The RFC requires the official distribution to carry a tested stdlib version
offline, while allowing independent releases. This ADR freezes the **bundled
first slice**. A separate `marqdo-stdlib` repository is intentionally deferred
by ADR-0039: the bundle stays in this repository while the architecture is
small and clear; the deterministic package remains extraction-ready.

## Decision

### 1. Bundle location

The official bundle is the `stdlib/` tree shipped next to the CLI (or vendored
at the repository root during development). Discovery order:

1. `MARQDO_STDLIB_DIR` — explicit override; if set and invalid, loading fails
   hard (no silent fallback);
2. `<executable-dir>/stdlib` if it exists;
3. the development tree `<repo>/stdlib` for source builds.

If no bundle is found, a project that uses `std:` fails with a clear error; a
project that does not use `std:` is unaffected. No network, Git or package
manager is consulted for a bundled stdlib.

### 2. Explicit `std:` imports only

Front-matter imports may name a stdlib module as:

```yaml
import:
  io: "std:io"
  fmt: "std:format"
  parser: "std:data/json"
```

The target after `std:` is a **package-relative** path inside the bundle.
`..`, absolute paths and `std:` targets that escape the bundle are rejected;
`.mq.md` is appended when the target has no extension. `std:` is resolved
before relative/alias imports and never searches the project or dependencies.
There is no implicit `std` namespace, no global name, and no auto-import.

Module ids use a stable package instance:

```text
std:marqdo-stdlib@<version>#<sha256-hex>
```

so the same logical module from a different bundle is a different instance.

### 3. Content digest

`stdlib/stdlib.json` carries the name, version and digest. The digest is:

```text
sha256( for each sorted *.mq.md relative path:
          path_utf8, NUL, file_bytes, NUL )
```

It excludes `stdlib.json`, README files and non-`.mq.md` files. The loader
recomputes it before loading any stdlib module and fails closed on mismatch or
a missing manifest. A bundle is therefore identified by **name + version +
exact markdown bytes**, not by a path or a version string alone.

### 4. Visibility and analysis

Analysis v2 records the verified bundle identity under `project.stdlib`:

```json
{"version":"0.1.0","digest":"sha256:<hex>",
 "instance_id":"std:marqdo-stdlib@0.1.0#<hex>"}
```

It is `null` when no bundle was available. A project that imports `std:` always
has a non-null identity. Trace/diagnostics keep the real stdlib source spans;
the bundle is normal Markdown, not a special-cased module type.

### 5. Frozen-CLI closure lane

The first closure test builds the CLI once, hashes the executable, runs a
project importing `std:io`, then re-hashes the executable and asserts equality.
It also asserts:

* the project uses only `.mq.md` and standard imports, no `foreign`/`plugin`
  calls;
* a tampered bundle fails the digest check before any module executes;
* Analysis records the verified digest.

This is a local gate, not a replacement for a remote CI lane.

### 6. Versioning

The bundle version is `0.1.0` for this slice. A CLI may load a bundle whose
version is declared compatible by this ADR; the RFC's full SemVer/digest
contract and independent release workflow remain CM-A10 follow-up. A future
core feature that the bundle requires must be reflected in both the bundle
version and the CLI's compatibility table before release.

## Non-goals

* Creating/publishing a separate `cflmy/marqdo-stdlib` repository now; see
  ADR-0039 for the deliberate deferral and future extraction triggers.
* Automatic download/update, Git fetching, or manifest-driven stdlib
  installation.
* A second plugin/provider system or a second module search path.
* Compat-B/C name routing of `打印` (ADR-0027) — the bundle does not change
  builtin resolution.
* Windows/macOS packaging layout; only the source-tree/executable-adjacent
  layout is exercised locally.

## Offline packaging

`tools/pack-stdlib.py` produces a deterministic `marqdo-stdlib-<version>.tar.gz`
plus a schema-tagged descriptor (`tree_digest`, `archive_sha256`, file list) for
the future separate repository/release workflow. The archive is not loaded by
the CLI; the in-repo `stdlib/` tree remains the verified development/bundled
source. Publishing/signing the external repository is still an external step.

## Consequences

* Any project can `import: {io: "std:io"}` and use the print prototype
  offline; `打印` remains the default builtin.
* A changed stdlib file without a digest update is rejected before execution.
* The bundle is still an in-repo vendored package. Independent repository
  release and CI signing are explicitly tracked as remaining CM-A10 work.
* Analysis artifacts and a frozen binary can be tied to the exact stdlib
  digest, which is the prerequisite for Compat-B and reproducible releases.

## Verification

`docs/evidence/2026-11-20-bundled-stdlib-cm-a10.md` records the `std:` import
test, digest-tamper rejection, Analysis identity assertion, no-foreign policy
scan and frozen binary hash equality.
