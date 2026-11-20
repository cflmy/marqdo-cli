# ADR-0039: Stdlib extraction is intentionally deferred

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A10, P2/P3), ADR-0028
Supersedes: the assumption in ADR-0028/CM-A10 evidence that a separate
`marqdo-stdlib` repository must be created as the next step.

## Context

The RFC and the first CM-A10 slice anticipated a separate `marqdo-stdlib`
repository so the standard library could be versioned and released independently
from the CLI. After implementing the bundled tree, `std:` imports, digest
identity, frozen-binary closure, table/HTTP modules and the deterministic
offline package, the current architecture is clear and the bundle is still
small. A separate repository would add release, version-mapping, signing and
cross-repo CI overhead without a present consumer that needs an independent
release cadence.

This decision **does not delete** the extraction work or the packaging artifact;
it changes the sequencing/intent.

## Decision

1. **Current policy: one repository.** `stdlib/` stays in `marqdo-cli` as the
   official, verified bundle. `std:` imports, the SHA-256 tree digest,
   module-instance ids and `project.stdlib` Analysis identity are unchanged.
2. **Extraction remains an explicit future option.** The deterministic package
   (`tools/pack-stdlib.py`), `marqdo.stdlib-package` descriptor and version
   mapping remain maintained as the extraction-ready artifact.
3. **No current release work is blocked on a separate repo.** Documentation,
   STATUS and evidence must not describe "publish the external stdlib repo" as
   a pending P2/CM-A10 gate; it is an intentionally deferred follow-up.
4. **Extraction triggers** (any one requires a new ADR before splitting):
   * the bundle grows enough that CLI release cadence and stdlib cadence
     conflict;
   * a non-CLI consumer needs an independent stdlib release/SemVer line;
   * multiple implementations or a provider/package ecosystem need version
     skew management;
   * the stdlib needs a different review/ownership boundary or signing policy.
5. **Compatibility if extraction happens:** keep `std:` import spelling,
   module ids, digest algorithm and Analysis `project.stdlib` shape; publish a
   version mapping and enforce the same frozen-binary/tamper gates.

## Non-goals

* Deleting the packaging script or historical CM-A10 evidence.
* Changing `std:` semantics, bundle digest or module ids now.
* Creating a package registry, signing infrastructure or cross-repo CI while
  there is no independent consumer.
* Declaring the stdlib frozen forever; the trigger list above is intentional.

## Consequences

* The repository remains the single source of truth and the dev loop stays
  simple.
* "Not external" is no longer reported as a missing gate or blocker.
* A future split can reuse the deterministic archive and digest contract
  without redoing the bundling design.

## Verification

Docs/STATUS/evidence reference this ADR; `cargo test --workspace --locked`
continues to exercise the bundled `std:` imports, digest and frozen-CLI lane.
