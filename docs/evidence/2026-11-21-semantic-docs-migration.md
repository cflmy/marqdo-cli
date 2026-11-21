# 2026-11-21 — RFC-LANG-0002 documentation and test-text migration

Status: **partial evidence**. This records the repository-wide *text* migration
to the single syntax, not an implementation of the frontend. Semantic and
negative suites are expected red; see the tail of this file.

- Repository: `cflmy/marqdo-cli`
- Worktree base: `0d8ed44` (`feat(http): parse and build HTTP/1.1 framing by bytes`),
  plus uncommitted migration edits in the worktree (this evidence file is part
  of those edits; no commit SHA can pin the uncommitted tree).
- The worktree also contains an **in-progress, partially applied frontend
  migration** (e.g. heading-mode/Knowledge-unit plumbing) that is outside this
  documentation migration. Only the commands below were observed; a later
  worktree state may legitimately differ.
- Machined: Linux x86_64, Rust 1.100.0.
- Authority: [`../architecture/RFC-LANG-0002-unified-semantics.md`](../architecture/RFC-LANG-0002-unified-semantics.md),
  [`../adr/0052-unified-markdown-semantics.md`](../adr/0052-unified-markdown-semantics.md).

## What was migrated

| Scope | Result |
|---|---|
| `spec/mapping.md`, `spec/expression.ebnf` | Replaced in place with the unified mapping/statement grammar. |
| `spec/evaluation.md`, `references.md`, `markdown-profile.md`, `security.md`, `math.md`, `foreign.md`, `package-format.md`, `plugin-abi.md`, analysis/trace notes | Updated for heading modes, payload origin, `sem.*` diagnostics, and in-place examples. |
| `README.md`, `AGENTS.md`, `COMPATIBILITY.md`, `CHANGELOG.md`, `docs/STATUS.md` | Rewritten to the current syntax with an explicit "spec ahead of implementation" state. |
| `docs/architecture/*`, `docs/migration/*`, `docs/adr/*` | New RFC/ADR/index, migration guide + inventory, historical banners on superseded decisions. |
| Active guides and planning docs | Rewritten or explicitly marked Historical/Superseded; old examples retained only as archives. |
| `examples/**` (15 `.mq.md`) | Migrated; example lock digests regenerated with the real CLI. |
| `tests/conformance/semantics/**` (73 `.mq.md`) | Migrated; expected value goldens unchanged. |
| `tests/negative/**` | 45 hard-error, 2 warning-model, 21 runtime fixtures for the new syntax/diagnostics; genuine runtime negatives retained. |
| `tests/security/run-lane.sh` | Embedded fixtures migrated. |
| Rust-embedded `.mq.md` literals in `crates/**/*.rs` | ~121 embedded programs migrated; no frontend semantics implemented. |
| `stdlib/**/*.mq.md` | **Not migrated** — tracked as pending with the frontend; `stdlib/README.md` says so. |

Coverage and remaining items:
[`../migration/semantic-rewrite-inventory.tsv`](../migration/semantic-rewrite-inventory.tsv).

## Commands and results

```text
cargo fmt --all -- --check
  PASS (exit 0)

cargo check --workspace --tests --locked
  PASS — Finished dev profile in 41.10s (string/fixture edits compile;
  no semantic implementation was added)

cargo test -p marqdo-cli --test conformance_ast
  4 passed, 0 failed
  (the Markdown profile and 160 AST goldens are unchanged)

cargo test -p marqdo-cli --test docs_consistency
  4 passed, 0 failed
  (one overview, resolvable links in the checked set, SHA-pinned evidence)

cargo test -p marqdo-cli --test conformance_semantics
  FAILED at tests/conformance/semantics/01-constant-return.mq.md:
  error[expr.parse]: invalid return expression: unexpected integer literal `1`
  after expression
  (the pre-RFC expression parser does not know the `return` keyword)

cargo test -p marqdo-cli --test negative
  FAILED: warning goldens expect sem.inferred_signature (not emitted by the
  pre-RFC frontend); compile negatives panic/emit old diagnostics; runtime
  fixtures parse as Knowledge under the pre-RFC mapper, so they "succeed" with
  Unit instead of reaching their runtime errors.

cd examples/packages-multi-version && ../../target/debug/marqdo lock
  locked 2 package(s); lock file equal to the migrated expectations
  (content digests verified with the real Rust file-set hash)
```

These failures are the expected "spec ahead of implementation" state recorded
in [`../STATUS.md`](../STATUS.md) §0, not evidence that the migrated fixtures
are invalid under the target semantics. The real closure requires implementing
RFC-LANG-0002 in `marqdo-frontend` / `marqdo-expression`, migrating
`stdlib/**/*.mq.md`, regenerating `stdlib.json`, and then re-running the full
gate.

## Explicit non-claims

- No new syntax is claimed executable by the current binary.
- No platform, security, ABI or protocol result is refreshed by this migration.
- No golden was regenerated to make a red suite pass.
- The `markdown.inert_executable` warning model is retired; the current
  warning/error contract is `sem.*` in `spec/mapping.md` §8.
