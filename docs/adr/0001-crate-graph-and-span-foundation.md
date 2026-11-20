# ADR-0001: Crate graph and the `marqdo-span` foundation

Status: **Accepted**
Date: 2026-11-20

## Context

`docs/DEVELOPMENT_BLUEPRINT.md` §11.1 fixes the dependency direction for the
workspace and states: "`markdown ->` zero other Marqdo crate; responsible for a
stable view of the standard AST." The implemented workspace instead has
fifteen crates, and `marqdo-markdown` depends on `marqdo-span`.

Why the divergence was necessary:

* Every layer needs identical byte spans: the Markdown AST
  (`crates/marqdo-markdown/src/ast.rs`), expression AST
  (`crates/marqdo-expression`), IR (`crates/marqdo-ir`), diagnostics and runtime
  errors (`crates/marqdo-value`), effect traces (`crates/marqdo-host`), plugin
  errors (`crates/marqdo-plugin-abi`) and the CLI renderer.
* A `Span` must be the same type across boundaries or every layer needs
  conversion functions. Conversions are exactly where off-by-one bugs and
  broken source locations appear, and they would make `Expr`/`Stmt`
  serde output inconsistent between `inspect --ast` and `inspect --ir`.
* The blueprint's "zero Marqdo dependencies" rule exists to keep the parser
  independent of the semantic layers. A tiny, effect-free span crate preserves
  that property while satisfying the shared-data-contract requirement.

## Decision

1. Add `crates/marqdo-span` as a **leaf** crate with no Marqdo dependencies and
   only `serde` as an external dependency. It defines:
   * `Span { start: usize, end: usize }` — half-open byte range into the
     original UTF-8 source, with `merge`, `shift`, `contains`, `len`;
   * `LineCol { line, column }` — 1-based, byte columns;
   * `LineIndex` — precomputed line starts for byte→line/column conversion.
2. Treat `Span` as part of the toolchain's shared data contract. Crates that
   produce or consume source locations depend on `marqdo-span` directly; they
   do **not** wrap it in local span types.
3. `marqdo-markdown` depends on `marqdo-span` (and `comrak`) but on **no
   semantic crate**. It still owns only the canonical AST view, front matter
   extraction and source maps; it performs no mapping, resolution or execution.
4. Keep the rest of the blueprint's direction:
   `value -> span`, `expression -> span`, `ir -> span + expression`,
   `markdown -> span + comrak`, `frontend -> markdown + expression + ir + value`,
   `host -> value + math + foreign + sandbox`, `runtime -> ir + value + host +
   expression`, `cli -> all` (composition root). There is no `runtime -> cli`
   edge and no cycle; `cargo tree` is expected to stay acyclic.
5. Do not introduce the blueprint's separate `sema` or `environment` crates in
   M0.5: semantic analysis lives in `marqdo-frontend`
   (`analyze`, `calls`, `infer`, `signature`, `units`, `map`), and package/
   environment formatting lives in `marqdo-package` plus the CLI. Splitting
   them is a refactor, not a semantic change.

Actual dependency graph (workspace `Cargo.toml` plus crate manifests):

```text
span            serde
value           span, serde, serde_json, num-bigint, num-traits
expression      span, serde, num-bigint, unicode-ident
markdown        span, comrak, serde, serde_yaml, unicode-normalization
ir              span, expression, serde
frontend        span, markdown, expression, ir, value, sha2,
                unicode-normalization, unicode-ident
math            value, num-bigint, num-rational, num-traits, serde
foreign         span, value, serde_json, thiserror
sandbox         serde
plugin-abi      span, value, serde, serde_json, serde_yaml, thiserror,
                libloading, sha2, hex
plugin-sdk      plugin-abi, serde, serde_json
host            span, value, math, foreign, sandbox, serde, serde_json,
                thiserror, glob
runtime         span, value, ir, expression, host, num-bigint,
                num-rational, num-traits, thiserror
package         serde, serde_yaml
cli             span, value, expression, markdown, ir, frontend, math,
                foreign, plugin-abi, host, runtime, package, sandbox,
                clap, serde, serde_json, walkdir, hex, sha2
```

## Alternatives

1. **Put `Span` in `marqdo-value`.** Then `marqdo-markdown` would depend on the
   runtime value model, which is a semantic layer and pulls in `num-bigint` and
   `serde_json`. Worse layering than a dedicated leaf.
2. **Duplicate `Span` in each crate and convert at boundaries.** Avoids a crate
   but guarantees conversion code, mismatched serde shapes and duplicated
   tests; rejected.
3. **Define `Span` in `marqdo-markdown` and let others depend on it.** Makes
   the parser a de-facto foundation crate and lets non-Markdown layers depend
   on `comrak`'s dependencies; rejected.
4. **Use a third-party range type.** Adds a dependency and gives no benefit
   over ~170 lines; rejected.
5. **Keep the blueprint's literal "markdown has zero Marqdo deps".** Would
   force either duplication or a span type in the parser that cannot be shared
   with diagnostics; rejected in favor of a documented, narrow exception.

## Markdown AST shape

No syntax change. Every `MdNode` still has `span: Span` in the original file
(`crates/marqdo-markdown/src/ast.rs`), and the JSON inspection view is
`{"kind":"heading","level":2,"span":{"start":0,"end":7},...}`. Spans are
half-open byte offsets, BOM-rebased, and byte columns for diagnostics.

## Compatibility

* This is a **new repository**; there is no legacy compatibility burden.
* Public Rust API: moving `Span` later is a workspace-wide API change but has
  no language/ABI/package-format impact. It would require updating every
  `crates/*/Cargo.toml`.
* Stable observable contracts (AST JSON `span`, diagnostic JSON `span`, IR
  `span`) are unchanged by the crate boundary. Changing span semantics
  (e.g. line/column units) requires a spec change and ADR.
* The divergence from blueprint §11.1 is recorded here; future crates should
  use `marqdo-span` rather than introducing another location type.

## Security

No new capability or effect. `marqdo-span` is `serde`-only, performs no I/O and
cannot execute anything. Byte offsets are derived from already-parsed source;
`LineIndex::line_col` clamps out-of-range offsets instead of panicking.

## Testing

* `crates/marqdo-span/src/lib.rs` tests multi-byte UTF-8 line/column mapping,
  `merge`/`shift`/`contains`, and clamping.
* `crates/marqdo-markdown/src/profile.rs` tests BOM rebasing and that every AST
  node carries a byte span; frontend tests assert diagnostics point at the
  expected source line/column.
* `cargo test --workspace` must pass; `cargo tree` must remain acyclic (no
  automated CI gate yet — this is a P7 item).

## Migration & rollout

Immediate: the crate exists and all layers already use it. Future work may
split the frontend into `sema`/`environment` crates; that must preserve the
graph above (`runtime` must never depend on `cli`, `markdown` must never depend
on semantic crates). If a second span representation is ever needed (for
example for generated code), add it as a conversion in `marqdo-span`, not as a
per-crate duplicate.
