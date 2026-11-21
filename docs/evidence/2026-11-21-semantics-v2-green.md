# 2026-11-21 — RFC-LANG-0002 implementation gate

Status: **green for code commit `9c1723d` on Linux x86_64**.
This supersedes the intermediate "text migrated, implementation pending" state
recorded in [`2026-11-21-semantic-docs-migration.md`](2026-11-21-semantic-docs-migration.md).

- Repository: `cflmy/marqdo-cli`, branch `master` (the former
  `semantics-v2-refactor` branch was fast-forwarded to `master` and deleted)
- Code commit: `9c1723d` (the documentation-sync commit that contains this file
  is docs-only and directly follows it)
- Series: `4983542` docs/spec, `e029853` breaking implementation, `ffd1cc9`
  expression statement grammar, `bbeb1de` status/evidence, then follow-up fixes
  `b38b769`, `361a6cc`, `d5002ac`, `9fb97bc`, `a3f9770`, `0bbe0f6`, `b3429cb`,
  `9c1723d`
- Toolchain: Rust 1.100.0, Linux x86_64
- Authority: [`../architecture/RFC-LANG-0002-unified-semantics.md`](../architecture/RFC-LANG-0002-unified-semantics.md),
  [`../adr/0052-unified-markdown-semantics.md`](../adr/0052-unified-markdown-semantics.md)

## Commands and results

```text
cargo fmt --all -- --check
  PASS

cargo clippy --workspace --all-targets -- -D warnings
  PASS

cargo test --workspace --locked --no-fail-fast
  PASS: 507 passed, 0 failed, 2 ignored
  (62 test binaries/doctest targets all `ok`)

python3 tools/pack-stdlib.py
  PASS: stdlib.json digest matches the migrated tree
```

Targeted suites relevant to the language rewrite:

```text
conformance_ast           4 passed   (160 AST goldens, profile unchanged)
conformance_semantics     1 passed   (73 fixtures, values/stdout preserved)
negative                  3 passed   (47 hard-error + 2 warning + 21 runtime)
properties                passed     (prose/container invariance)
integration_cli           21 passed  (init/check/run/call/inspect/fmt/lock)
stdlib_product            16 passed  (migrated bundled stdlib)
p2_closure                passed     (hash table/allocator fixtures in pure mode)
docs_consistency          4 passed
```

## What the implementation covers

- heading `@exec`/`@knowledge` declarations with ancestor inheritance; only an
  Exec H2 is callable; H2 Knowledge and H3–H6 sections never become functions;
- unified `Strong`/`Emph`/inline `Code` payloads, single ownership, nested
  carrier rejection, empty-payload rejection and precise UTF-8 spans;
- a separate statement grammar in `marqdo-expression` for
  `if/else/while/end/return/params`, reserved words and payload-relative spans;
- frontend control-stack lowering to the existing `Branch`/`Loop`/`Return` IR,
  with heading-segment boundaries and no cross-heading `end`;
- `params(...)` only as the first direct Exec H2 payload; existing free-variable
  inference (named arguments only) otherwise;
- `sem.*` diagnostics including mode marker, unit name/duplicate, exec without
  function, payload nesting/empty, syntax, unmatched/duplicate else/end,
  unclosed control, signature position, unsupported version directive,
  inferred-signature lint and `run`'s no-entry-function error; emitted in the
  documented stable order (source position, then code);
- migrated `stdlib/**/*.mq.md` with a regenerated `stdlib/stdlib.json` digest;
- migrated examples, semantics/negative/runtime corpus and Rust-embedded
  fixtures.

## Follow-up fixes included in the green commit

After the first green gate, the concurrency leftovers were reviewed and fixed:

* `params(...)` position is function-wide: a later `### @exec` segment can no
  longer add a signature after earlier statements (negatives 48–49);
* signatures come only from Exec payloads — a `params(...)` code span in a
  Knowledge section is inert and no longer becomes `inspect --signature` output;
* a stray `end` diagnostic points at the `end` payload, not the previous
  statement;
* Setext headings inherit but never declare a mode, as the spec requires;
* diagnostics are sorted by `(span.start, code)` for stable IDE/CI output;
* the dead `Stmt::Call`/`CallSite`/`CallTarget` IR representation and the
  `normalized_heading_name` helper from the removed link-form call are gone.

## Not claimed by this evidence

- Windows and macOS native test lanes were **not** run from this checkout
  (`UNVERIFIED`); no remote CI URL exists for `9c1723d`.
- The disposable-VM strict security lane was **not** re-run at this SHA. The
  in-tree `marqdo-sandbox` security tests and the workspace suite pass, but
  that is not the same evidence.
- No release tag is cut; release gates and platform evidence remain governed by
  [`../STABLE_RELEASE_IMPLEMENTATION.md`](../STABLE_RELEASE_IMPLEMENTATION.md)
  and [`../security-gate.md`](../security-gate.md).
