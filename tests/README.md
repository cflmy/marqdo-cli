# Marqdo conformance and integration test corpus

This directory holds the P0/P1 conformance corpus required by
`docs/DEVELOPMENT_BLUEPRINT.md` §13/§14. The Rust harnesses live in
`crates/cli/tests/` (they use the workspace libraries directly and the
`marqdo` binary via `CARGO_BIN_EXE_marqdo`); the fixtures are plain files here
so they can be rendered, diffed and reviewed like Markdown documents.

Everything here is data. Nothing in this directory modifies the repository, and
no test commits, copies or rewrites files outside a throw-away temp directory.

## Layout

```text
tests/
├── conformance/
│   ├── ast/            160 × *.mq.md + sibling *.ast.json goldens
│   └── semantics/       44 × *.mq.md + sibling *.expected.json goldens
├── negative/
│   ├── *.mq.md          52 × compile-time hard-error fixtures + *.expected.json
│   ├── warnings/         6 × inert-executable warning fixtures + *.expected.json
│   └── runtime/         12 × runtime-error fixtures + *.expected.json
└── README.md           this file
```

Rust harnesses (all in `crates/cli/tests/`):

| Harness | What it proves |
|---|---|
| `conformance_ast.rs` | The frozen `marqdo.md.v1` profile parses each fixture into exactly the checked-in AST (node kinds, attributes, literals, byte spans, order). |
| `conformance_semantics.rs` | Each semantic fixture compiles through `marqdo_frontend`, is assembled into `marqdo_ir::Program` and executed with `marqdo_runtime` + `marqdo_host::StdHost`; returned wire value and program stdout match. |
| `negative.rs` | Invalid executable shapes are **hard errors** (never prose fallbacks), exact sorted diagnostic-code lists; warnings and runtime error codes are asserted in their own suites. |
| `properties.rs` | Blueprint §14.2 prose invariance: inserting/removing pure narration paragraphs never changes the executable function IR (modulo source spans) and diagnostics stay error-free. |
| `integration_cli.rs` | Real CLI contract: `init`, `check`, `run`, `call`, `inspect`, `fmt`, multi-module imports/cycles, `lock`/`sync --locked`, plugin inspection and explicitly trusted plugin invocation. |

## Running

Use the workspace cargo cache/target as documented in `AGENTS.md`:

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target

cargo test -p marqdo-cli            # all conformance + integration harnesses
cargo test --workspace              # whole workspace (includes crate unit tests)
```

Individual harnesses:

```bash
cargo test -p marqdo-cli --test conformance_ast
cargo test -p marqdo-cli --test conformance_semantics
cargo test -p marqdo-cli --test negative
cargo test -p marqdo-cli --test properties
cargo test -p marqdo-cli --test integration_cli
```

The power/control-flow slice is pinned by:

* AST fixtures 151–160: code-span vs tight/spaced `**`, multi-backtick spans,
  inert prose/code fences, branch/loop headers and math blocks;
* semantic fixtures 37–44: right-associative power, unary-minus precedence,
  parentheses, float negative exponents, Strong+CodeSpan assignment, branch,
  loop and inert prose;
* negative fixtures 49–52: missing power/condition operands and executable
  nodes swallowed by a condition paragraph;
* runtime negatives 11–12: negative integer exponent and exponent budget;
* `crates/marqdo-frontend/src/payload.rs` span tests and
  `crates/cli/tests/integration_cli.rs` CLI span/E2E assertions.

Fixture discovery is directory-based and sorted by file name, so the order is
deterministic on every platform. The AST and semantic suites fail loudly if the
corpus shrinks below 100 / 25 fixtures, and the negative suite fails below 30
compile-time hard-error fixtures.

## AST goldens and `UPDATE_GOLDENS`

`*.ast.json` files are the pretty-printed canonical AST view
(`marqdo_markdown::parse(source).root`), exactly as `marqdo inspect --ast --json`
would print it. They pin parser version, extension flags and span arithmetic.

Regenerate **only** for an intentional, ADR-backed profile change (the profile
is frozen in `spec/markdown-profile.md`):

```bash
UPDATE_GOLDENS=1 cargo test -p marqdo-cli --test conformance_ast
```

The update run rewrites every golden, prints how many it rewrote and **passes**.
Re-run without the variable to verify, and review the golden diff: a golden
change is a language change, not a bug fix.

A fixture without a golden is an error, and a golden without a fixture is an
error (stale corpus output), so add/remove both together.

## Semantic goldens

Each `*.expected.json` has the shape:

```json
{
  "function": "<name passed to marqdo_runtime::run>",
  "args": { "<name>": <wire value> },
  "value": <wire value>,
  "stdout": "..."
}
```

* `args` and `value` use the typed wire encoding from `marqdo_value::to_wire`
  (`{"type":"int","decimal":"42"}`, `{"type":"text","value":"…"}`, …).
* `stdout` is the program output captured from the host (never diagnostics).
* Optional `"grants": ["foreign"]` enables `foreign.call`; that fixture is
  skipped with a printed notice when `python3` is unavailable.

All values were produced by running the implementation and then checked by
hand against the fixture source. `UPDATE_SEMANTICS_GOLDENS=1` exists for
authoring convenience, but it must not be used blindly: rewriting a semantic
golden without verifying the value would defeat the purpose of the corpus.

## Negative fixtures

Compile-time (`tests/negative/*.mq.md`):

```json
{"codes": ["markdown.invalid_construct"]}
```

The expected list is the exact sorted multiset of **error** diagnostic codes
produced by `marqdo_frontend::analyze_source`. Warnings emitted by the same
document are ignored here. Every listed code is additionally asserted to have
`error` severity, and every fixture must produce at least one error — invalid
executable shapes never fall back to prose.

Warnings (`tests/negative/warnings/*.mq.md`):

```json
{"severity": "warning", "codes": ["markdown.inert_executable"]}
```

These documents must compile **without errors**; the sorted multiset of warning
codes must match exactly. `return`/`strong`/lists/block quotes outside a
function body are inert content (warning), not hard errors.

Runtime (`tests/negative/runtime/*.mq.md`):

```json
{"function": "main", "args": {}, "codes": ["runtime.division_by_zero"]}
```

These documents compile, then must fail at runtime with the expected stable
error code. Optional keys: `"grants": ["foreign"]` and `"stack_limit": 24`
(used for the recursion-depth fixture so the budget — not the host process
stack — is what fails).

## CLI integration notes

`integration_cli.rs` never leaves the repository dirty:

* temporary projects are created under `std::env::temp_dir()` and removed on
  drop;
* the demo C plugin is built **in a temp copy** of `examples/plugin-demo`
  (`cc -shared -fPIC -I <repo>/include`), and the `sha256` in the copied
  `plugin.yaml` is refreshed there. The repo copy and its `.so` (gitignored) are
  never touched;
* tests that need `python3` or `cc` print a skip notice and return when the tool
  is unavailable.

## Notes discovered while authoring (all since fixed)

The fixtures initially pinned three observations that were reported and then
fixed in the implementation; the goldens below reflect the fixed behaviour:

* wire floats round-trip exactly (`serde_json/float_roundtrip` is enabled), so
  Python `math.pi * 9` keeps `28.274333882308138`;
* the default interpreter stack budget is 128 frames so a deeply recursive
  document returns a structured `resource.exhausted` instead of exhausting the
  native debug-build stack (`--stack` can raise it deliberately);
* an external-link call form (`> [x](https://example.com)()`) is rejected with
  `ref.unresolved`, the same code used for other unresolvable call targets.
