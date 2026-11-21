# Marqdo Evaluation Semantics v1

Status: **frozen for M0.5; extended by ADR-0022 (`Memory`) and ADR-0024 (recoverable results)**.
Changes to observable behaviour require an ADR.

## 1. Values

| Type | Meaning |
|---|---|
| `Unit` | no result; a function that falls through returns `Unit` |
| `None` | explicit empty value; distinct from `Unit` |
| `Bool` | `true` / `false`; the only type usable in conditions |
| `Int` | arbitrary precision integer (`num-bigint`) |
| `Float` | IEEE-754 binary64 |
| `Text` | Unicode string |
| `List` | ordered values |
| `Map` | string-keyed record with deterministic (sorted) key order |
| `Table` | header names + rows of values (from GFM tables) |
| `Formula` | inert parsed LaTeX + provenance; math operations are explicit |
| `Code` | inert code fence + provenance; execution is explicit |
| `FunctionRef` | reference to a named function in a module |
| `PluginHandle` | opaque host-owned plugin export handle |
| `Memory` | opaque runtime-owned addressable byte region (ADR-0022); copies share identity, offsets are region-relative |
| `Resource` | opaque host-issued resource/stream handle (ADR-0025); not indexable, not wire-transferable, rendered as `Resource` |

Read-only observation members:

| Value | Members |
|---|---|
| Formula | `latex`, `display`, `unit` |
| Code | `language`, `source`, `content_hash`, `unit` |
| FunctionRef | `module`, `name` (ADR-0029) |
| PluginHandle | `plugin`, `export` (ADR-0029); the numeric host id stays private |
| Memory, Resource | no fields (opaque) |

There is no truthiness: `if`-like conditions accept `Bool` only. There are no
implicit string↔number conversions; conversion builtins must be called.

## 2. Numeric semantics

* `Int` arithmetic (`+ - * // % **`) is exact. `/` always produces `Float`.
* Mixed `Int`/`Float` arithmetic yields `Float`; the conversion of an `Int`
  larger than 2^53 may lose precision and this is documented, not silently
  exported as a rounded `Int`.
* Comparisons between `Int` and `Float` are **exact** (the float is treated as
  the exact binary rational it is); no lossy comparison is performed.
* `//` and `%` require `Int` operands. Floor division: `a // b` is the
  mathematical floor; `a % b == a - b * (a // b)` (sign follows divisor).
* Division/modulo by zero is `runtime.division_by_zero`.
* `+` also concatenates two `Text` values. No other operator is overloaded.
* `**` on `Int` uses exact exponentiation; a negative exponent is a type error
  (use `/` or `Float` operands for fractional results). Exponents above a
  configurable budget are `resource.exhausted`.
* Floats may be `NaN`/`±Infinity` only as results of operations; they are
  tagged explicitly on the wire. NaN compares unequal to everything.

## 3. Evaluation order and scopes

* Blocks execute in Markdown AST document order; executable inline nodes
  execute in source order within a paragraph.
* A function call evaluates its arguments in source order, exactly once each,
  then binds them to fresh parameter slots in an isolated frame. The callee
  cannot see the caller's locals (no dynamic scoping); imported bindings are
  read-only.
* Assignment binds/updates a local slot in the current frame. It never writes
  an outer frame. There is no module-level assignment in v1: assignment
  outside a function body is an error.
* The only other writable place is an index into a `Memory` region
  (`m[offset] = byte`, ADR-0022). List, Map and Table values keep their v1
  value semantics; `list[i] = value`, `map[key] = value` and `obj.field = value`
  are not defined and are rejected at run time or parse time rather than
  silently changing those values into shared mutable objects.
* For `m[offset] = byte` the operands are evaluated exactly once in the order
  **base, offset, right-hand side**. The target must evaluate to `Memory`, the
  offset to a non-negative `Int` inside the region and the value to an `Int`
  in `0..=255`; otherwise the statement fails without writing anything.
* `and`/`or` short-circuit and require `Bool` operands.
* `Return` (an `emph` node) unwinds through branch/loop bodies to the
  enclosing function frame.
* `Branch` checks conditions in AST order and executes the first `true` branch
  once. `Loop` re-evaluates its condition before each iteration.
* Imported modules only register definitions for the importing document; they
  have no initialization side effects and their `main` is never started.
* Function calls recurse; recursion depth is bounded by the stack budget.
  Mutual recursion is allowed.

### 3.1 Addressable `Memory` (ADR-0022)

`Memory` is a separate storage domain from managed `List`/`Map`/`Table`
values. It exists so `.mq.md` programs can construct allocators, encodings and
data structures without a new Markdown node, keyword or host operation.

* **Creation.** `memory.new(initial_pages, max_pages)` creates a zero-filled
  region and returns its handle. Both arguments are non-negative `Int`s;
  `initial_pages <= max_pages`, and `max_pages` must fit the per-region
  budget. Rejection is atomic: no region or committed byte is created.
* **Page size.** One logical page is 64 KiB (`PAGE_SIZE`). Print, trace and
  storage use bytes; page counts never mix with byte offsets.
* **Size and growth.** `memory.size(memory)` returns committed pages.
  `memory.grow(memory, pages)` is grow-only and returns the **old** page
  count; old offsets keep their bytes, every new byte is zero. A failed grow
  (declared maximum, region budget or total budget) leaves the region and the
  budget unchanged. Growing by zero pages is a no-op returning the current
  count. Regions never shrink and there is no user-visible `free(region)`.
* **Read/write.** `m[offset]` returns an `Int` in `0..=255`; `m[offset] = byte`
  writes one byte. Offsets are non-negative and region-relative; negative,
  out-of-range and unrepresentable `BigInt` offsets are `runtime.index`.
  Out-of-range byte values are `runtime.type`. No implicit truncation or
  wrapping occurs.
* **Identity and equality.** Copying a `Memory` value, passing it to a
  function or storing it in a collection shares the same region. Two handles
  are `==` only when they designate the same region instance; `Memory` has no
  ordering comparison and no byte-content equality operator.
* **Lifetime.** A region lives for one run and is dropped with the runtime.
  Block-level ownership (`malloc`/`free`) is a standard-library concern; the
  runtime only guarantees that a single access stays inside its region.
* **Diagnostics.** Unknown, stale or cross-run handles are
  `runtime.memory`; budget failures are `resource.exhausted`; a non-`Memory`
  index-assignment base is `runtime.type`.
* **Boundary.** A live handle never crosses the wire: `to_wire` emits only
  `{"type":"memory"}`, decoding that tag is a `WireError::Invalid`, and every
  host/plugin/foreign call rejects arguments containing `Memory`.
* **Display.** `Value::display()` renders `Memory` without the internal
  region identity; `类型(m)` returns `"Memory"`.

### 3.2 Recoverable results (ADR-0024)

A recoverable failure is an ordinary `Map`, not a new value type or syntax:

```text
success: {ok: true,  value: <any Value>}
failure: {ok: false, code: <Text>, message: <Text> [, details: <Map>]}
```

`ok` is always a `Bool`. Success carries `value`; failure carries a stable
dot-separated `code` and a human-readable `message`. Constructors such as
`成功`/`失败` are ordinary `.mq.md` functions, not builtins.

* Expected conditions (no free block, malformed parser input, invalid byte
  sequence, and later EOF/short I/O results) should use this shape so a caller
  can branch on `ok`.
* Language-invariant violations (wrong type, out-of-bounds, unknown/stale
  `Memory` handle, budget exhaustion, capability denial) remain fatal
  `RuntimeError`s. There is no catch mechanism and a library must not try to
  convert them into `Err`.
* `map.get` returning `None` is a lookup result, not this convention; use
  `map.has` to distinguish a stored `None` (ADR-0023).
* `code` is stable within its library; callers must not branch on `message`.
  Recommended prefixes include `alloc.*`, `parse.*`, `codec.*`, `table.*` and,
  later, `io.*`.
* Stream EOF/partial semantics belong to the resource ABI (CM-A06); this
  section only reserves the shape `{ok: true, value: {count, eof, ...}}` for
  later, without defining it.

### 3.3 Resource streams (ADR-0025)

`Resource` is a host-issued opaque handle to an external stream, in a separate
domain from `Memory`. It is not indexable, not a byte address and cannot be
constructed from a number, document or wire value.

```text
resource.open(kind: Text, descriptor: Text, rights: List) -> Resource
resource.read(handle, memory: Memory, offset: Int, max_bytes: Int)
    -> {ok:true, value:{count:Int, eof:Bool}} | {ok:false, code, message}
resource.write(handle, memory: Memory, offset: Int, length: Int)
    -> {ok:true, value:{count:Int}} | {ok:false, code, message}
resource.close(handle)
    -> {ok:true, value:None} | {ok:false, code, message}
resource.accept(listener)
    -> {ok:true, value:Resource} | {ok:false, code, message}
resource.request(operation: Text, arguments: List) -> Result
```

* Providers: `stdio/stdin` with `["read"]` and `stdio/stdout`/`stdio/stderr`
  with `["write"]` (pure/project; one shared stdin cursor and separate stdout
  and stderr buffers; console grant). Strict permits only the existing
  `stdio/stdout` channel. `file` with `["read"]` (project, `fs_read` grant,
  bounded by the 8 MiB file limit); `file` with `["write"]` (project,
  `fs_write` grant, 8 MiB buffered write flushed on `close`, ADR-0026).
  Mixed rights and other kinds are `resource.unsupported`.
* `resource.read` checks the destination `Memory` range before calling the
  provider, caps the request to the remaining buffer, writes the returned bytes
  and reports the actual count plus `eof`. EOF is success, not an error.
* `resource.write` copies bytes out of `Memory` for the call only; a short write
  is a count. stdout/stderr output is charged to the output budget and delivered
  to separate program channels. A project-file write stream appends to a bounded
  host buffer and does not touch the filesystem until `close`; a failed close
  leaves the handle open with its buffer, and an unclosed writer never creates
  the file.
* Reading/writing/closing an already closed handle returns
  `{ok:false, code:"resource.closed"}`. Forged/cross-run/stale handles are
  fatal `runtime.resource`; unsupported kind/rights/direction are
  `resource.unsupported`; capability/path denial is `permission.denied`; a
  writer buffer above 8 MiB is `resource.exhausted` and appends atomically.
* `Memory` arguments never cross the host boundary as handles: the interpreter
  copies bytes for the call. Resource handles never cross the value wire.

### 3.4 Resource operations (ADR-0041)

`resource.request(operation, arguments)` is the non-streaming control plane for
operations that are not byte streams. This build registers only `fs.*`:

```text
resource.request("fs.exists",   [path])        -> {ok:true, value:Bool}
resource.request("fs.metadata", [path])        -> {ok:true, value:{kind, size}}
resource.request("fs.list",     [path])        -> {ok:true, value:List<Text>}
resource.request("fs.mkdir",    [path])        -> {ok:true, value:None}
resource.request("fs.remove",   [path])        -> {ok:true, value:None}
resource.request("fs.rename",   [from, to])    -> {ok:true, value:None}
resource.request("fs.append",  [path, text])    -> {ok:true, value:count}
resource.request("fs.read_at", [path, offset, len]) -> {ok:true, value:Text}
resource.request("fs.walk",    [dir])           -> {ok:true, value:List<Text>}
resource.request("clock.now",   [])             -> {ok:true, value:UnixMillis}
resource.request("env.get",     [name])         -> {ok:true, value:Text}
resource.request("random.bytes", [count])       -> {ok:true, value:List<Int>}
resource.request("compress.gzip",   [text])     -> {ok:true, value:List<Int>}
resource.request("compress.gunzip", [bytes])    -> {ok:true, value:Text}
resource.request("regex.match",    [pattern, text]) -> {ok:true, value:Bool}
resource.request("regex.captures", [pattern, text]) -> {ok:true, value:{matched, groups}}

resource.request("process.run", [program, args])-> {ok:true, value:{status, stdout, stderr}}
```

* Arguments are `Text` paths only. `Memory`/`Resource` values are rejected.
* Project mode only; `pure`/`strict` are `permission.denied`. Grants:
  `exists`/`metadata`/`list` need `fs_read`; `mkdir`/`remove` need `fs_write`;
  `rename` needs `fs_write` on both paths. Grants are checked before path
  resolution or I/O.
* Paths follow the project-file policy: relative, no `..`, no absolute path,
  canonical target inside the root.
* `kind` is `"file"`, `"directory"` or `"other"`; `size` is the file byte size
  and `0` for non-files. `list` returns names sorted by their text form.
* Expected conditions are recoverable: `fs.not_found`, `fs.not_directory`,
  `fs.exists` (`mkdir`/`rename` target exists) and `fs.not_empty`
  (`remove` on a non-empty directory). Unknown operations, traversal,
  capability denial and I/O failures stay fatal.
* `remove` never recurses; `rename` never overwrites; `mkdir` is single-level.
* `append` writes UTF-8 text to the end of a file (creating it) and returns the
  written count; `read_at` returns at most `len` bytes from `offset` as Text;
  `walk` recursively lists regular files under a directory, skipping symlinks,
  with depth 64 / 4096-entry limits (ADR-0045).
* The operation never moves file bytes. Content still uses
  `resource.open/read/write/close`.
* `clock.now` takes no arguments and returns UTC Unix milliseconds. It is
  project-mode only like `fs.*`; a clock before the epoch is fatal host error
  (ADR-0042).
* `env.get` takes one exact variable name and requires an `--allow-env NAME`
  grant; un-granted names are `permission.denied`, missing names are recoverable
  `env.not_found` and non-UTF-8 values are `env.not_unicode` (ADR-0043).
* `regex.match`/`regex.captures` use the host regex engine (groups,
  alternation, character classes, captures); invalid patterns are recoverable
  `regex.invalid`. They are project-mode only; the pure `.mq.md` subset remains
  available (ADR-0050).
* `compress.gzip`/`compress.gunzip` codec UTF-8 Text through a byte List;
  input/output are capped at 8 MiB and bad gzip/UTF-8 is recoverable
  `compress.invalid`/`compress.not_utf8` (ADR-0049).
* `random.bytes` takes an Int count in `1..=65536`, requires
  `--allow-random`, and returns a List of 0–255 OS entropy bytes (ADR-0046).
* `process.run` takes an exact program path and a List of Text arguments,
  requires an `--allow-process PROGRAM` grant, runs without a shell with a
  cleared environment in the project root, and enforces a 5 second timeout and
  1 MiB stdout/stderr caps. It is project-mode only and is **not** an OS
  sandbox (ADR-0044).

## 4. Errors

All failures are structured. Compile-time problems produce diagnostics
(`markdown.invalid_construct`, `expr.parse`, `symbol.unknown`, …). Runtime
problems produce `RuntimeError { category, span, message, frames, cause }`
with a stable category code. Runtime errors stop the current function and
propagate with a call stack. A host/foreign/plugin failure never panics the
interpreter; it becomes a structured error. Addressable-memory failures use
the stable codes `runtime.memory` (unknown/stale handle, invalid page counts),
`runtime.index` (offset outside the region, including negative or huge
offsets), `runtime.type` (wrong operand type or byte value outside `0..=255`)
and `resource.exhausted` (region or run budget).

## 5. Parameter inference (static, name based)

Parameters are discovered by definite-assignment data-flow analysis over the
function's control-flow graph, never from prose:

1. Build the CFG of the function body in AST order, with branch and loop
   edges. Loop bodies may execute zero times.
2. Compute the set of variables *definitely assigned* on every path from the
   function entry.
3. Every read of a variable that is not definitely assigned at that point and
   that has no assignment anywhere in the function is a **candidate
   parameter** (sorted by canonical name).
4. If a variable is assigned on some paths but read before that assignment on
   others, its identity is ambiguous: emit `symbol.uninitialized` (S-04)
   instead of inventing a parameter.
5. Exclude builtins, imported aliases and reserved names from inference.
6. A read that is not explained by a parameter, a local, a builtin or an
   import is `symbol.unknown` with a hint (typo protection).

Consequences: `**结果 = n * 阶乘(n=n-1)**` infers `n`; `**n = 3**` followed by
reads makes `n` a local. Inferred parameters are named arguments only; an
explicit signature paragraph allows positional arguments.

## 6. Resource budgets

Every run has explicit limits (all configurable, all reported as
`resource.exhausted`): evaluation fuel, call stack depth, total output bytes,
wall-clock time, host process count, and addressable memory. Addressable
memory has two independent budgets (ADR-0022): total committed bytes across
all regions (`DEFAULT_MAX_TOTAL_BYTES = 64 MiB`) and committed bytes per
region (`DEFAULT_MAX_REGION_BYTES = 16 MiB`, hence at most 256 pages of
64 KiB), overridable with `--max-memory` and `--max-region-memory`. Memory is
a pure computation: it needs no capability grant and is available in `pure`,
`project` and `strict` modes, subject to these budgets.
Loops consume fuel per iteration and expressions per node so that a hostile
document cannot hang the CLI. Strict mode additionally enforces limits at the
OS level where available; if the platform cannot provide that, strict mode
refuses to run (see `security.md`).

## 7. Determinism

For a fixed source, mode, capabilities and budget, evaluation is
deterministic. Stdout/stderr from host operations are separated: program
stdout and program stderr are captured separately, diagnostics/traces go to the
CLI stderr, and `--json` output never mixes with any program output. JSON
reports `stdout` and `stderr` as separate strings.

## 8. Builtins v1

Builtins are part of the language surface and are capability-checked at the
host boundary. Names are stable; adding one requires a spec update.

| Name | Signature | Notes |
|---|---|---|
| `打印` / `print` | `(x) -> Unit` | Compat-C: routed to bundled `io.写值`; no `HostOp` formatting path. Without the bundle, `kernel.unavailable` |
| `长度` / `len` | `(x) -> Int` | Text scalar count, List/Map/Table size |
| `类型` / `type` | `(x) -> Text` | value type name |
| `文本` / `str` | `(x) -> Text` | explicit conversion |
| `整数` / `int` | `(x) -> Int` | from Int/Float(if integral)/Text(decimal) |
| `小数` / `float` | `(x) -> Float` | from Int/Float/Text |
| `表` / `table` | `(header: List, rows: List) -> Table` | validates row widths |
| `ref` | `(target: Text) -> FunctionRef/Formula/Code` | resolves a document address; module-local, refuses ambiguous resource units (`ref.ambiguous`) |
| `memory.new` | `(initial_pages: Int, max_pages: Int) -> Memory` | creates a zero-filled region; budgets are checked before allocation (ADR-0022) |
| `memory.size` | `(memory: Memory) -> Int` | committed page count |
| `memory.grow` | `(memory: Memory, pages: Int) -> Int` | grow-only; returns the previous page count, new bytes are zero |
| `list.append` | `(list: List, value) -> List` | new list with one appended value; input unchanged (ADR-0023) |
| `list.concat` | `(left: List, right: List) -> List` | new concatenated list; inputs unchanged |
| `map.keys` | `(map: Map) -> List` | Text keys in canonical sorted order |
| `map.has` | `(map: Map, key: Text) -> Bool` | key membership |
| `map.get` | `(map: Map, key: Text) -> Value` | stored value or `None`; use `map.has` when `None` is a legal value |
| `map.set` | `(map: Map, key: Text, value) -> Map` | new map with the key; input unchanged |
| `map.remove` | `(map: Map, key: Text) -> Map` | new map without the key; missing key is a no-op |
| `text.codepoints` | `(text: Text) -> List` | Unicode scalar values as `Int`, in order |
| `text.from_codepoint` | `(codepoint: Int) -> Text or None` | `None` for a surrogate or a value outside `0..=0x10FFFF` |
| `table.columns` | `(table: Table) -> List` | header names in column order |
| `table.row` | `(table: Table, row: Int) -> List` | new list of row cells; non-negative coordinate |
| `table.cell` | `(table: Table, row: Int, column: Int) -> Value` | cell value; non-negative coordinates |
| `resource.open` | `(kind: Text, descriptor: Text, rights: List) -> Resource` | capability-checked stream open (ADR-0025) |
| `resource.read` | `(handle, memory: Memory, offset: Int, max_bytes: Int) -> Result` | bounded chunk read into Memory; `{count,eof}` |
| `resource.write` | `(handle, memory: Memory, offset: Int, length: Int) -> Result` | bounded byte write; `{count}` |
| `resource.close` | `(handle) -> Result` | releases the host slot |
| `resource.accept` | `(listener) -> Result` | accepts one TCP connection from a `tcp-listener` handle (ADR-0047) |
| `resource.request` | `(operation: Text, arguments: List) -> Result` | non-streaming `fs.*` operations (ADR-0041) `clock.now` (ADR-0042), `env.get` (ADR-0043), `process.run` (ADR-0044) and `random.bytes` (ADR-0046); project-mode and grant-checked |
| `math.parse/eval/substitute` | see `spec/math.md` | P5; `math.simplify`/`math.differentiate` are declared but return `math.unsupported` in this build |
| `foreign.run/call` | see `spec/foreign.md` | P5, capability-gated |
| `fs.read/write` | see `spec/security.md` | capability-gated |

`打印/print` is **Compat-C** (ADR-0031): the legacy `HostOp::Print` and its
`Value::display()` formatting path are removed. When a loaded module calls the
builtin, the loader injects `stdlib/io.mq.md` and sets a program-level
`print_override`; the runtime calls `io.写值`. The stdlib chain formats every
one of the 15 value types, encodes UTF-8 into `Memory`, and writes via
`resource.write(stdout)`. If the official bundle is unavailable, the builtin
returns a structured `kernel.unavailable` error; there is no silent formatting
fallback. The builtin name remains for arity validation and diagnostics.

The `list.*`, `map.*`, `text.*` and `table.*` entries are observation and
construction primitives, not algorithms (ADR-0023). They return new values and
never mutate their arguments. Sorting, filtering, joining, grouping, JSON/CSV
and table transformations remain standard-library work: adding one must not
add a builtin. `map.get` returns `None` for a missing key; `map.has` is the
unambiguous membership check when a stored `None` is possible. `table.row` and
`table.cell` use non-negative coordinates, while `List` indexing keeps its v1
negative-index rule.
