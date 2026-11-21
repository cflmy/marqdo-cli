# Marqdo conformance and integration test corpus

This directory holds the P0/P1 conformance corpus required by
RFC-LANG-0002 and `docs/DEVELOPMENT_BLUEPRINT.md` §13/§14. The Rust harnesses live in
`crates/cli/tests/` (they use the workspace libraries directly and the
`marqdo` binary via `CARGO_BIN_EXE_marqdo`); the fixtures are plain files here
so they can be rendered, diffed and reviewed like Markdown documents.

Everything here is data. Nothing in this directory modifies the repository, and
no test commits, copies or rewrites files outside a throw-away temp directory.

> **Migration state (RFC-LANG-0002).** The source text in this corpus uses the
> current single syntax. The Rust frontend still implements the pre-RFC mapping,
> so the semantic, negative, property and CLI suites that execute Marqdo source
> are **expected to be red** until the frontend migration lands. `conformance_ast`
> is unaffected by the semantic change (the Markdown profile is unchanged).
> `tests/negative/warnings/01-inferred-signature` pins the optional
> `sem.inferred_signature` lint as enabled; the current frontend does not emit
> it. `sem.no_entry_function` is a `run` entry-selection diagnostic and cannot be
> pinned by `crates/cli/tests/negative.rs`, which only calls `analyze_source` /
> `marqdo_runtime::run`; that coverage needs a CLI integration test. Do not
> regenerate goldens or restore the old mapper to turn these suites green. See
> [`../docs/STATUS.md`](../docs/STATUS.md) and
> [`../docs/migration/README.md`](../docs/migration/README.md).

## Layout

```text
tests/
├── conformance/
│   ├── ast/            160 × *.mq.md + sibling *.ast.json goldens
│   └── semantics/       73 × *.mq.md + sibling *.expected.json goldens
├── negative/
│   ├── *.mq.md          45 × compile-time hard-error fixtures + *.expected.json
│   ├── warnings/         2 × warning-model/control fixtures + *.expected.json
│   └── runtime/         21 × runtime-error fixtures + *.expected.json
└── README.md           this file
```

Rust harnesses (all in `crates/cli/tests/`):

| Harness | What it proves |
|---|---|
| `conformance_ast.rs` | The frozen `marqdo.md.v1` profile parses each fixture into exactly the checked-in AST (node kinds, attributes, literals, byte spans, order). |
| `conformance_semantics.rs` | Each semantic fixture compiles through `marqdo_frontend`, is assembled into `marqdo_ir::Program` and executed with `marqdo_runtime` + `marqdo_host::StdHost`; returned wire value and program stdout match. |
| `negative.rs` | Current-syntax diagnostics are **hard errors** (never prose fallbacks): exact sorted `sem.*`/`symbol.*`/`call.*`/`frontmatter.*` code lists, the `sem.inferred_signature` warning model with inert-knowledge controls, and runtime error codes — each asserted in its own suite. |
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
* negative fixtures 18–23: malformed payload statements (missing condition,
  trailing colon, `end if`, empty `return`, multi-statement payload, bad
  `params` syntax); fixtures 24–29 pin control-stack pairing, including the
  heading boundary that reports `sem.unclosed_control` and `sem.unmatched_end`
  independently;
* runtime negatives 11–12: negative integer exponent and exponent budget;
* `crates/marqdo-frontend/src/payload.rs` span tests and
  `crates/cli/tests/integration_cli.rs` CLI span/E2E assertions.

The addressable-memory slice (RFC-CM P1, ADR-0022) is pinned by:

* semantic fixtures 45–47: sharing a region by handle, per-region isolation,
  `memory.size`, `memory.grow` returning the old count, preserved old bytes and
  zero-filled new pages;
* runtime negatives 13–18: out-of-bounds and huge offsets, byte values outside
  `0..=255`, index assignment on a `List`, growth beyond the declared maximum
  and negative page counts;
* `tests/negative/runtime/15-index-assign-non-memory.mq.md` also guards the
  current value semantics of `List` (it must not become shared mutable memory);
* `crates/marqdo-runtime/src/memory.rs` unit tests (aliasing, atomic failed
  growth, forged/stale handles, page arithmetic) and
  `crates/marqdo-runtime/tests/interpreter.rs` host-boundary/budget tests;
* `crates/marqdo-value/src/wire.rs` proves live handles are not transferable;
* `examples/memory-buffer.mq.md` and the `examples_r6` CLI test prove the
  `.mq.md` surface end to end.

Partial RFC-CM P2 (compute closure) is demonstrated, not certified, by:

* semantic fixtures 48–50: a big-endian u32 codec and bump allocator, three
  allocated nodes linked and traversed, and a failed allocation that leaves
  the allocator header unchanged. These documents use only language builtins
  and control flow; no Rust algorithm is added for them.
* The full P2 gate also needs a free-list allocator with reuse/coalescing,
  tree/graph structures, a recoverable `Result` model and an independent
  stdlib repository. Those are **not** implemented.

CM-A04 structure primitives (ADR-0023) are pinned by:

* semantic fixtures 51–54: value semantics of `list.append`/`map.set`/
  `map.remove` (inputs unchanged, keys sorted, `map.has`/`map.get`),
  Unicode scalar round-trip through `text.codepoints`/
  `text.from_codepoint` (including invalid scalars → `None`),
  `table.columns`/`table.row`/`table.cell` projection, and a `.mq.md`
  `过滤` function that rebuilds a filtered GFM-style table without mutating
  the input or adding a Rust table algorithm;
* runtime negatives 19–21: `list.append` on a Map, table row out of bounds,
  and a non-Text `map.get` key;
* `crates/marqdo-runtime/src/builtins_impl.rs`; no algorithm builtin such as
  sort/filter/join/JSON/CSV is added.

CM-A05 recoverable results and the first-fit allocator (ADR-0024) are pinned
by:

* semantic fixture 55: a `.mq.md` first-fit/free-list allocator with 8-byte
  block headers, free-block reuse, block splitting, address-adjacent
  coalescing (a second region fills its bump space, so only the merged block
  can satisfy a 40-byte request), a too-large request returning
  `{ok:false, code:"alloc.no_space", ...}`, and payload writes that show the
  reused blocks do not overlap;
* the `Result` shape is an ordinary Map; no `Result` value type, exception,
  `try`/`catch` or propagation syntax is added. The ADR records the
  success/failure field contract and the fatal-vs-recoverable boundary;
* semantic fixture 56: a `Memory`-backed binary search tree with 16-byte nodes,
  recursive insertion and recursive in-order traversal built from
  `list.concat`; the returned root/left/right offsets and sorted values pin
  node layout and traversal;
* semantic fixture 57: a `.mq.md` CSV single-row splitter using
  `text.codepoints`, `text.from_codepoint`, `list.append` and `list.concat`;
  it preserves empty fields and does **not** pretend to support quoted or
  escaped CSV;
* semantic fixture 58: an open-addressing hash table with linear probing,
  collision handling (keys 5 and 37), value update, not-found `None`, element
  count, and a full-table `hash.full` Result in a second region;
* semantic fixture 59: a `.mq.md` UTF-8 encoder that writes 1–4 byte sequences
  into `Memory` from `text.codepoints`, returning the byte length and bytes;
* semantic fixture 60: a validating `.mq.md` UTF-8 decoder over `Memory` with
  explicit `utf8.truncated`, `utf8.invalid_continuation`, `utf8.invalid_scalar`
  and `utf8.invalid_lead` failures; valid ASCII, 2/3/4-byte and mixed text is
  decoded;
* semantic fixture 61: a `Memory`-backed graph adjacency list with 16-byte nodes
  and 8-byte edges, head-inserted edges and a neighbor traversal; no Rust graph
  container or new `Value` type;
* semantic fixture 62: a recursive `.mq.md` JSON parser/formatter for nested
  arrays and objects, strings with standard escapes including `\uXXXX`,
  ints and floats (including exponents), `true`/`false`/`null`, whitespace,
  `next` positions, compact output and `Result` errors
  (`json.unexpected_eof`, `json.invalid_array`, `json.invalid_object`,
  `json.invalid_string`, `json.invalid_number`, `json.invalid_literal`,
  `json.unexpected_token`);
* semantic fixture 63: a `Memory`-backed stack bytecode VM with instruction
  decode, PUSH/ADD/MUL/JMP/JZ/HALT, a stack region and two programs (arithmetic
  and conditional branch); no Rust VM or interpreter loop was added;
* semantic fixture 64: `.mq.md` big-endian 16/32/64-bit encoders/decoders and
  LEB128 varint encode/decode, including the `u64` maximum and byte-level
  round-trips;
* semantic fixture 65: a `.mq.md` table sort built on `table.row`,
  `table.columns`, `table.cell`, `表` and `list.append`; insertion sort
  rebuilds a new table and leaves the input table unchanged;
* semantic fixture 67: a `.mq.md` table equality join over two tables, using
  `table.row/columns/cell`, `list.concat`, `list.append` and `表`; both input
  tables remain unchanged;
* semantic fixture 68: a quoted/escaped CSV single-row parser using Unicode
  scalar primitives; `""` is a literal quote and commas inside quotes are
  preserved. Multi-line fields and whitespace trimming are out of scope;
* semantic fixture 69: table group by a Text key column into a `Map` of value
  lists, using `table.row/cell`, `map.has/get/set` and `list.append`;
* semantic fixture 70: Int matrix multiply over list-of-list row-major data,
  covering C12's layout/iteration shape without a numeric backend;
* fixture 55 additionally has a second-free detection path: freeing an
  already-free block returns `{ok:false, code:"alloc.double_free"}`.

P5 TCP endpoint v1 (ADR-0034) is pinned by two host tests in
`crates/marqdo-host/tests/resource_streams.rs` (loopback echo with a grant;
grant/pure/strict/DNS refusal) and
`crates/cli/tests/resource_streams.rs::tcp_stream_round_trips_through_mqmd_with_a_network_grant`
(`--allow-net 127.0.0.1`).

RFC-CM CM-A09 C ABI worker binding (ADR-0038) is pinned by
`crates/marqdo-provider-worker/tests/worker_binding.rs`, which compiles a C
provider, runs it through the worker helper and the host transport, and checks
missing-library fail-closed startup.

RFC-CM CM-A09 C ABI provider (ADR-0036) is pinned by
`crates/marqdo-host/tests/c_provider.rs`, which compiles a small C shared
library, registers it, round-trips `resource.*`, checks missing-library,
wrong-ABI-version and pure/strict fail-closed behavior; the package-manifest
test installs a C provider from `provider.json`.

RFC-CM CM-A09 worker provider transport (ADR-0035) is pinned by
`crates/marqdo-host/tests/worker_provider.rs`: a Python helper is spawned,
pinged, opened/read/written/closed through framed JSON; missing grant, pure,
strict and missing-program cases are rejected. The helper is trusted, not a
sandbox.

RFC-CM CM-A09 in-process provider v1 (ADR-0033) is pinned by the three
`registered_provider_streams_*` / `provider_*` tests in
`crates/marqdo-host/tests/resource_streams.rs`: a mock provider is registered
and read/written through `resource.*` with a project-mode name grant; missing
grant, pure/strict, bad protocol version, reserved kinds and duplicate kinds
are rejected.

RFC-CM P2 closure (C01–C06, C11, C12) is pinned by
`tools/pack-stdlib.py` is the CM-A10 deterministic package builder (not a test;
its descriptor is checked in under `docs/evidence/`).

`tools/perf/bench.py` is the P6 baseline harness (not a test; it is not part of
the cargo gate).

`crates/cli/tests/p2_closure.rs`: 21 semantic fixtures run through
`run --mode pure --json`, sources are scanned for foreign/plugin/import
shortcuts, and value/stdout are compared with the goldens; the stdlib table
self-test also runs in pure mode. Negative cases prove `foreign.call` is
`permission.denied` and `plugin.invoke` fails in the pure lane.

RFC-CM CM-A08 table stdlib (ADR-0032) is pinned by
`crates/cli/tests/stdlib_table.rs`, which runs `stdlib/tests/table.mq.md` and
checks select/filter/sort/append/set/join/group results plus original-table
immutability.

RFC-CM CM-A07 Compat-C (ADR-0031) is pinned by
`crates/marqdo-runtime/tests/interpreter.rs::print_without_the_bundled_stdlib_is_a_structured_error`
and the absence of any `HostOp::Print`/`op_print` reference. The semantic
harness mirrors the loader's stdlib injection for direct-runtime fixtures.

RFC-CM CM-A07 Compat-B (ADR-0030) is pinned by
`crates/cli/tests/print_routing.rs`:

* a print program's trace contains `resource.write` and no `print` effect;
* stdlib modules are injected only for programs that call `打印/print`;
* all existing semantic/CLI print goldens still pass byte-identically.

RFC-CM CM-A10 bundled stdlib (ADR-0028) is pinned by
`crates/cli/tests/stdlib_bundle.rs` (6 tests):

* explicit `std:io` import resolves the verified bundle and runs;
* Analysis records `project.stdlib.version/digest/instance_id`;
* a tampered `io.mq.md` is rejected with `stdlib digest mismatch` before any
  module executes;
* a missing manifest is a hard error;
* the bundle has no `foreign.`/`plugin.` calls;
* the CLI binary hash is unchanged after running a `std:io` project
  (frozen-CLI closure).

RFC-CM CM-A07 Compat-A (ADR-0027) is pinned by:

* `stdlib/{format,text,io}.mq.md` and `stdlib/demo.mq.md`;
* `crates/cli/tests/examples_r6.rs::mqmd_stdlib_print_output_matches_builtin_print`,
  which runs the demo and asserts `io.写值` and builtin `打印` lines are
  byte-identical for a nested List/Map/None value and a Table;
* the demo covers Unit/None/Bool, Int, Float, Text, List, Map, Table,
  Formula, Code and FunctionRef; PluginHandle formatting uses the same
  observation members but needs a plugin to construct a value.
  `打印` is routed to the stdlib by ADR-0030 Compat-B when the bundle is
  present; this demo still calls the explicit `io.写值` to compare paths.

RFC-CM CM-A06 resource streams (ADR-0025) are pinned by:

* semantic fixture 66: `.mq.md` UTF-8 text encoded into `Memory`, written to a
  `stdio/stdout` resource, closed, then re-used to prove
  `{ok:false,code:"resource.closed"}`; the expected `stdout` is the decoded
  text;
* `crates/marqdo-host/tests/resource_streams.rs` (17): pure-mode stdin read,
  stdout/stderr separation, output-budget atomicity, strict refusal of
  stdin/stderr/file, project-file chunking/EOF, missing grant, traversal
  rejection and unsupported rights/kinds;
* `crates/cli/tests/resource_streams.rs`: real `marqdo run` with
  `--allow-read data.txt`, three chunked reads into `Memory` plus EOF, piped
  stdin line reads through `std:io/console`, separate stdout/stderr capture,
  the same document failing with `permission.denied` without the grant, and a
  write-then-read program with `--allow-write/--allow-read out.bin` proving the
  file is created only at `resource.close`, and a chunked `.mq.md` file copy
  (`resource.read` → `resource.write`) that reproduces `data.txt` in `out.bin`;
* `crates/marqdo-host/src/resource.rs` unit tests for forged/closed handles,
  stdout read/file write unsupported, and chunked EOF.

Fixture discovery is directory-based and sorted by file name, so the order is
deterministic on every platform. The AST and semantic suites fail loudly if the
corpus shrinks below 100 / 25 fixtures; the warning and runtime suites fail if
their directories are empty; the negative suite fails below 30 compile-time
hard-error fixtures.

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

All three suites use the single current language contract
(`docs/architecture/RFC-LANG-0002-unified-semantics.md`,
`spec/mapping.md`, `spec/expression.ebnf`): `@exec`/`@knowledge` heading
modes, interchangeable `Strong`/`Emph`/inline-`Code` one-statement payloads,
and explicit `if`/`else`/`while`/`end`/`return`/`params`. Ordinary Markdown
lists, block quotes, tables and emphasis are **not** errors: they are inert
containers/knowledge with an empty execution projection. The v1
`markdown.invalid_construct` shape errors ("ordered list must have a
condition", "block quote must be a call", "Strong must not nest Emph", "empty
heading") were removed with the old mapping; `markdown.inert_executable`
warnings no longer exist either.

Compile-time (`tests/negative/*.mq.md`, 45 fixtures):

```json
{"codes": ["sem.unclosed_control"]}
```

The expected list is the exact sorted multiset of **error** diagnostic codes
produced by `marqdo_frontend::analyze_source`. Warnings emitted by the same
document are ignored here. Every listed code is additionally asserted to have
`error` severity, and every fixture must produce at least one error — an
invalid Exec payload never falls back to prose.

Coverage by group:

| Group | Fixtures | Codes |
|---|---|---|
| heading modes and unit identity | 01–07, 44–45 | `sem.invalid_mode_marker`, `sem.empty_unit_name`, `sem.duplicate_unit_name`, `sem.exec_without_function` |
| payload extraction | 08–17 | `sem.payload_nested` (nested `Emph`/`Strong`/`Link`/math/strikethrough/line break), `sem.empty_payload` (whitespace-only `Strong`/`Emph`) |
| statement grammar | 18–23 | `sem.syntax` |
| control stack and heading boundary | 24–29 | `sem.unmatched_else`, `sem.duplicate_else`, `sem.unmatched_end`, `sem.unclosed_control` |
| signatures | 30–32 | `sem.signature_position` |
| version selection | 33–34 | `sem.unsupported_version_directive` |
| kept non-`sem` errors | 35–43 | `symbol.unknown`, `call.arity`, `symbol.uninitialized`, `frontmatter.invalid` |

Fixture 29 crosses a heading boundary on purpose: the opener's segment reports
`sem.unclosed_control` and the later `### @exec` segment reports
`sem.unmatched_end`; both are asserted, and neither is "repaired" across the
boundary (`spec/mapping.md` §7).

`sem.no_entry_function` is a `run` entry-selection diagnostic: it is produced
when the CLI cannot find an Exec H2 `main`. `crates/cli/tests/negative.rs` only
calls `marqdo_frontend::analyze_source` and `marqdo_runtime::run`, and the
latter reports an unknown *function* for a missing name; there is no CLI-entry
suite in that harness, so this corpus cannot pin the code without a harness
change. It is recorded here as a known gap rather than replaced by a weaker
assertion (see `spec/references.md` §7 and `spec/mapping.md` §8).

### v1 → current reclassification

| v1 fixtures | v1 expectation | Current disposition |
|---|---|---|
| 01–08 nested `Strong`/`Emph` children | `markdown.invalid_construct` | kept as 08–15 with `sem.payload_nested` |
| 09–12 empty/parse-error carriers | `markdown.invalid_construct`, `expr.parse` | empty carriers kept as 16–17 (`sem.empty_payload`); the unqualified expression-parse cases are replaced by the explicit `sem.syntax` statements in 18–23 |
| 13–24 list/quote shape errors | `markdown.invalid_construct`, `expr.parse` | **removed**: ordinary lists/quotes are valid inert Markdown now; the positive control is `warnings/02-knowledge-and-containers-are-inert` |
| 25–27 link-form calls (external/cross-module/anchor) | `ref.unresolved`, `module.unresolved` | **removed**: link-form calls no longer exist; `ref.unresolved`/`module.unresolved` are now `check --refs`/loader diagnostics outside this harness (`spec/references.md` §7) |
| 28–29 duplicate function/namespace names | `markdown.invalid_construct` | kept as 44–45 with `sem.duplicate_unit_name` (duplicate canonical name, `spec/references.md` §2.3) |
| 30–31 empty function/namespace headings | `markdown.invalid_construct` | replaced by 04–05 with `sem.empty_unit_name` on an actual mode declaration |
| 32–40 unknown/call/signature/frontmatter errors | `symbol.unknown`, `call.arity`, `symbol.uninitialized`, `expr.parse`, `frontmatter.invalid` | kept 35–43 (`frontmatter.*`, `symbol.*`, `call.arity`, `symbol.uninitialized`); malformed-signature cases moved to `sem.syntax`/`sem.signature_position` (23, 30–32) |
| 41–48 frontmatter/link/assignment/power shapes | `frontmatter.invalid`, `expr.parse` | frontmatter cases kept as 40–42; link/quote/assignment shapes removed with the old mapping; power/condition cases replaced by 18–29 |
| 49–52 power/condition operands swallowed by a branch condition | `expr.parse`, `markdown.invalid_construct` | **removed**: executable nodes inside list items are now independent payloads. Replacement negatives are the `sem.syntax` (18–23) and control-stack (24–29) fixtures |

Warnings (`tests/negative/warnings/*.mq.md`, 2 fixtures):

```json
{"severity": "warning", "codes": ["sem.inferred_signature"]}
```

These documents must compile **without errors**; the sorted multiset of warning
codes must match exactly. Fixture 01 pins the `sem.inferred_signature` lint for
an Exec H2 that relies on free-variable inference. Fixture 02 is a
zero-diagnostic control: it proves that ordinary ordered lists, bullet lists
and block quotes are valid inert Markdown even inside an `@exec` section, and
that an explicit `params(...)` declaration (allowed after prose) suppresses the
inferred-signature warning. The lint is specified as **optional**
(`spec/mapping.md` §6, "may be enabled"); these fixtures pin it enabled, so if
an implementation leaves it off the warning suite — not the hard-error corpus —
is what must be revisited.

Runtime (`tests/negative/runtime/*.mq.md`, 21 fixtures):

```json
{"function": "main", "args": {}, "codes": ["runtime.division_by_zero"]}
```

These documents compile, then must fail at runtime with the expected stable
error code. Sources were migrated to the current syntax (explicit `## @exec`
functions, `params(...)` where a function has arguments, payload calls and
explicit `if`/`end`), but the genuine runtime expectations were preserved:
arithmetic type/zero-division, index/field/memory bounds, `call.not_callable`,
`call.arity` through a `ref` function value, non-Bool branch condition,
capability (`permission.denied` for a foreign kernel in pure mode), unsupported
math subset, recursion budget and the addressable-memory/collection errors.
Optional keys: `"grants": ["foreign"]` and `"stack_limit": 24` (used for the
recursion-depth fixture so the budget — not the host process stack — is what
fails).

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

Two observations were reported and then fixed in the implementation; the
goldens reflect the fixed behaviour:

* wire floats round-trip exactly (`serde_json/float_roundtrip` is enabled), so
  Python `math.pi * 9` keeps `28.274333882308138`;
* the default interpreter stack budget is 128 frames so a deeply recursive
  document returns a structured `resource.exhausted` instead of exhausting the
  native debug-build stack (`--stack` can raise it deliberately).

The old link-form block-quote call (`> [x](https://example.com)()`) no longer
exists: a Markdown link is a static reference, so a call is now an ordinary
expression payload (`spec/references.md` §6.2).
