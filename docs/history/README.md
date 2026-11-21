# Historical snapshots

> **Historical (archived).** Everything in this directory is frozen at a past
> commit and is **not** current state. In particular, the language contract moved
> to the breaking rewrite
> [RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md) (see
> [ADR-0052](../adr/0052-unified-markdown-semantics.md) and
> [`spec/mapping.md`](../../spec/mapping.md)). Old-syntax examples in these
> snapshots are kept **as archives** and are not current syntax; do not copy
> them as tutorials.

This directory holds plans, reviews and status snapshots that were true at a past
commit but are **not current state**. They remain for provenance and design
history; do not cite them as what the tree does now.

- [`STATUS-M0.5-snapshot.md`](STATUS-M0.5-snapshot.md) — M0.5 status snapshot.
- `../NEXT_PHASE_IMPLEMENTATION.md` — M0.6 plan, superseded by
  `../STABLE_RELEASE_IMPLEMENTATION.md`; the file stays under `../docs/` so
  existing links keep working and carries a Historical banner.
- `../CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md` — review baseline
  `0adebe4`, marked Historical in place.
- `../NEXT_RELEASE_CI_CONVERGENCE.md` — CI baseline `c9f01aa8`, marked
  Historical in place.
- The top-level `../Marqdo_CLI_Latest_Architecture_Audit_and_Refactor_RFC.md`
  was a byte-identical duplicate of
  `../architecture/Marqdo_CLI_Latest_Architecture_Audit_and_Refactor_RFC.md`;
  it was removed in the documentation migration. The canonical copy is the
  architecture one.

The current architecture overview is
[`../architecture/CURRENT.md`](../architecture/CURRENT.md); executed status is
[`../STATUS.md`](../STATUS.md).
