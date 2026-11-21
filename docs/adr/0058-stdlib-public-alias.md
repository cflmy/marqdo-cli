# ADR-0058: Official stdlib public aliases replace print magic

- Status: Accepted (2026-11-21)
- Implements: RFC-EXT-0001 §8 (PR-06)
- Related: ADR-0028/0030/0031/0039, ADR-0054

## Context

`打印`/`print` were implemented twice:

1. `crates/cli/src/loader.rs::program_uses_print` walked the entire AST, and if
   any function called `打印`/`print` it injected the bundled
   `stdlib/io.mq.md` module and set `Program.print_override` to its `写值`
   function;
2. `crates/marqdo-runtime/src/interpreter.rs` contained a `打印`/`print`
   special case that redirected to the override, while the builtin table also
   contained a placeholder `打印` entry.

Copying this pattern to the next official high-level function (formatting,
console input, …) would add one AST scanner and one runtime branch per name.
The RFC forbids that ("为标准库每个高级函数复制 print_override 模式"), while
also requiring byte-identical output and no new Host op.

## Decision

1. **Verified metadata.** The bundled `stdlib/stdlib.json` declares

   ```json
   "public_aliases": [
     {
       "public_names": ["print", "打印"],
       "target_module": "io.mq.md",
       "target_function": "写值",
       "expected_signature": "params(值)",
       "effects": ["host"]
     }
   ],
   "aliases_digest": "sha256:<64 lowercase hex>"
   ```

   `aliases_digest` is the SHA-256 of the canonical binary encoding: for each
   alias sorted by `(target_module, target_function)`, the module bytes, NUL,
   the function bytes, NUL, then every public name in sorted order followed by
   NUL. The same encoding is implemented by `tools/pack-stdlib.py` and
   `crates/cli/src/stdlib.rs::aliases_digest`; `stdlib.json` cannot be edited
   without updating the digest, and the loader verifies both digests before any
   module loads. Metadata is integrity-checked, not signed: a SHA-256 proves
   content equality, not publisher identity.
2. **Lazy, generic loading.** The loader reads the metadata, decides whether the
   program references any public alias name (a generic identifier scan, not a
   per-function hard code), and only then loads the declared target module.
   The target signature is checked against `expected_signature`; a mismatch is
   a load error. A program without `打印`/`print` keeps its previous module
   graph and startup cost.
3. **Ordinary Marqdo binding.** For each used alias the loader produces an
   `OfficialAlias { public_names, function, signature, owner_package,
   effects_requested }`; `project_prepare` registers it in the
   `CallableRegistry`. The binding pass targets
   `Marqdo { function: <stdlib instance>::io.mq.md::写值 }`. The runtime calls
   an ordinary Markdown function; it has no print branch.
4. **Reserved names stay reserved.** Official aliases may replace the builtin
   placeholder for the same public name. Third-party registry insertion rejects
   builtin *and* official-alias names (`ext.duplicate_callable`), so no plugin
   or imported module can claim `打印`/`print`, `std.*` or `marqdo.*`.
   `plugin.yaml` still rejects reserved export prefixes at manifest validation.
5. **No Host print operation.** Output continues to flow through
   `io.写值` → `format.显示` → UTF-8 bytes → `Memory` →
   `resource.write(stdout)`. There is no `HostOp::Print` and no second
   formatter. Unbundled embedders that call the placeholder builtin receive
   `kernel.unavailable`.
6. **Removed code.** `program_uses_print`, `stmts_use_print`,
   `expr_uses_print`, `Program.print_override`, the runtime redirect branch and
   the whole-AST print scanner are deleted. A structural test
   (`architecture_boundaries.rs::callable_binding_is_the_only_static_resolution_path`)
   fails if they reappear.

## Consequences

* `打印` and `print` are now ordinary calls in Analysis/Trace: the call target
  is the stdlib function, `state` is `function`, and no synthetic `builtin`
  target is reported for the alias.
* Output must stay byte-identical. `crates/cli/tests/stdlib_alias_routing.rs`
  plus the existing print/format/Trace goldens pin this; a diff is a blocking
  regression, not an acceptable "new design".
* A tampered alias table (metadata digest or expected signature) is rejected
  before module loading; `stdlib.json` and the `.mq.md` tree remain separately
  digest-pinned, and `tools/pack-stdlib.py` refuses to build an inconsistent
  bundle.
* The generic metadata path is the only place a future official high-level name
  needs to be declared locally. Any *new language syntax* still requires a
  language RFC; aliases cannot change `@exec`/`@knowledge` semantics.

## Rejected alternatives

* **Keep `print_override` and add a second alias for another function.**
  Rejected: it multiplies hard-coded scans/branches and violates RFC-EXT-0001
  §8 and §14.10.
* **Move the print formatter into the Host as a `Print` op.** Rejected: it
  would reintroduce an un-audited output path and duplicate the stdlib
  formatter; output is a resource write.
* **Let third-party packages register `print`/`打印` aliases.** Rejected:
  official public names are a language surface, not an extension registry.
* **Put the alias table in Rust `match` arms instead of metadata.** Rejected:
  the table must be versioned with the verified bundle so a stale/mixed bundle
  cannot run a different function.
