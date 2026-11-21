# 2026-11-21 — RFC-LANG-0002 implementation gate

Status: **green for the implementation commit `ffd1cc9` on Linux x86_64**.
This supersedes the intermediate "text migrated, implementation pending" state
recorded in [`2026-11-21-semantic-docs-migration.md`](2026-11-21-semantic-docs-migration.md).

- Repository: `cflmy/marqdo-cli`, branch `semantics-v2-refactor`
- Code commit: `ffd1cc9862f7fe6002b42f1210010c1f78671a95`
- Previous steps: `4983542` (docs/spec), `e029853` (breaking implementation)
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
  PASS: 501 passed, 0 failed, 2 ignored
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
  inferred-signature lint and `run`'s no-entry-function error;
- migrated `stdlib/**/*.mq.md` with a regenerated `stdlib/stdlib.json` digest;
- migrated examples, semantics/negative/runtime corpus and Rust-embedded
  fixtures.

## Not claimed by this evidence

- Windows and macOS native test lanes were **not** run from this checkout
  (`UNVERIFIED`); no remote CI URL exists for `ffd1cc9`.
- The disposable-VM strict security lane was **not** re-run at this SHA. The
  in-tree `marqdo-sandbox` security tests and the workspace suite pass, but
  that is not the same evidence.
- No release tag is cut; release gates and platform evidence remain governed by
  [`../STABLE_RELEASE_IMPLEMENTATION.md`](../STABLE_RELEASE_IMPLEMENTATION.md)
  and [`../security-gate.md`](../security-gate.md).
