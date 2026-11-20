# RFC-CM CM-A05 evidence: `Result` convention and first-fit free-list allocator

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0/P1/P2 changes.
Decision records: [ADR-0024](../adr/0024-recoverable-result-convention.md),
[ADR-0023](../adr/0023-minimal-structure-observation-and-construction.md)
Related: [P1](2026-11-20-memory-p1.md),
[P2 closure](2026-11-20-memory-p2-closure.md),
[structure API](2026-11-20-structure-api-p2.md).
Scope: CM-A05 convention, a `.mq.md` first-fit allocator, and the later P2
fixtures 56–59 (binary tree, CSV splitter, hash table, UTF-8 encoder). Full P2
gate still **not** passed.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                                        PASS
cargo clippy --workspace --all-targets -- -D warnings             PASS
cargo test --workspace --locked                                   PASS
  47 test binaries, 389 passed, 0 failed, 2 ignored (doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## `Result` convention (ADR-0024)

Success and failure are ordinary `Map` values:

```text
success: {ok: true,  value: <Value>}
failure: {ok: false, code: <Text>, message: <Text>[, details: <Map>]}
```

No `Result` value type, exception, `try`/`catch` or automatic propagation was
added. Expected conditions return this shape; wrong types, out-of-bounds,
invalid/stale handles, budget exhaustion and capability denial remain fatal
`RuntimeError`s.

## Allocator fixture 55

`tests/conformance/semantics/55-first-fit-free-list-allocator.mq.md` is a
pure `.mq.md` first-fit allocator over `Memory`:

* 8-byte block header: total block size (`u32`) and free-list next (`u32`);
* region header: free-list head at offset 0, bump pointer at offset 4;
* allocation rounds the request up to an 8-byte multiple;
* first fit walks the free list, splits a fitting block when the remainder is
  at least 16 bytes, otherwise unlinks it;
* no suitable free block falls back to the bump pointer;
* no space returns `{ok:false, code:"alloc.no_space", message:...}`;
* the free list is address-sorted; `释放` inserts a block and coalesces it
  with an address-adjacent successor and predecessor without erasing payload
  bytes.

Observed result (compact wire values):

```text
甲 (8 bytes)  -> {ok:true, value:16}
乙 (16 bytes) -> {ok:true, value:32}
丙 (8 bytes)  -> {ok:true, value:16}   # reuses 甲's freed block
丁 (4 bytes)  -> {ok:true, value:16}   # reuses the same block
戊 (16 bytes) -> {ok:true, value:32}   # reuses 乙's freed block
己 (64 bytes) -> {ok:true, value:56}
庚 (8 bytes)  -> {ok:true, value:56}   # split from 己
辛 (16 bytes) -> {ok:true, value:72}   # split remainder
壬 (8 bytes)  -> {ok:true, value:96}   # split remainder
癸 (8 bytes)  -> {ok:true, value:112}  # exact remainder
failure       -> {ok:false, code:"alloc.no_space",
                  message:"没有足够的空闲空间"}
payload bytes -> 44, 55, 66, 77 at 庚/辛/壬/癸
```

A second region isolates coalescing from bump allocation:

```text
甲2 8B, 乙2 8B, 丙2 8B -> payloads 16, 32, 48
fill 65472B            -> consumes the rest of the 64 KiB region
free 甲2, 乙2, 丙2     -> address-adjacent blocks merge to one 48-byte block
丁2 40B (need 48)      -> {ok:true, value:16}   # only the merged block fits
戊2 8B                 -> {ok:false, alloc.no_space}  # bump space exhausted
```

Without coalescing, `丁2` would see only three 16-byte fragments and the
exhausted bump pointer, so it would fail; the observed success at offset 16
proves the merge.

The payload addresses do not overlap:
`庚=56..71`, `辛=72..95`, `壬=96..111`, `癸=112..127`. The earlier bytes
`甲=16..23`, `乙=32..47`, `戊=32..47` show deliberate reuse after `释放`
(the block is not zeroed; callers must initialise a block they allocate).

`cargo test -p marqdo-cli --test conformance_semantics` passes with fixture 55
included. `cargo run -p marqdo-cli -- run
tests/conformance/semantics/55-first-fit-free-list-allocator.mq.md --json`
returns `success: true` with the checked-in expected value.

## Binary tree fixture 56

`tests/conformance/semantics/56-memory-binary-tree.mq.md` is a second
`.mq.md` structure over `Memory`:

* 16-byte nodes: one-byte value at offset 0, left child offset at 4, right
  child offset at 8; `0` is the empty child;
* region header at offset 0 is the bump pointer, initialised to 8;
* `插入` is recursive and writes the child link through its parent block;
* `遍历` is recursive in-order and builds the result with `list.concat`;
* inserted `[5, 3, 8, 1, 4, 7, 9]`, returned:

```text
root=8
in-order=[1,3,4,5,7,8,9]
root.left=24
root.right=40
```

No Rust tree/container algorithm, new `HostOp`, `Value` variant or Markdown
syntax was added. `cargo test -p marqdo-cli --test conformance_semantics`
passes with fixture 56 included.

## CSV row splitter fixture 57

`tests/conformance/semantics/57-csv-row-splitter.mq.md` splits a single CSV row
using only `.mq.md` and the CM-A04 primitives:

* `text.codepoints` enumerates Unicode scalar values;
* `text.from_codepoint` reconstructs each character;
* `list.append` builds the field list, `list.concat` appends the final field;
* comma is the separator; empty fields are preserved.

Observed:

```text
"甲,乙,,丙" -> ["甲","乙","","丙"]
""          -> [""]
"a,b"       -> ["a","b"]
```

Quoted fields, embedded commas, doubled quotes and line breaks are **not**
implemented. This is a C05 demonstration, not a claim that CSV is complete.

## Hash table fixture 58

`tests/conformance/semantics/58-memory-hash-table.mq.md` implements an
open-addressing hash table over `Memory`:

* 8-byte header: element count at 0, slot count at 4; slots from offset 8,
  8 bytes each (`key:u32`, `value:u32`);
* key `0` means empty, so business keys start at 1;
* hash is `key % capacity` with linear probing;
* existing keys update in place without incrementing the count; a full table
  returns `{ok:false, code:"hash.full", ...}`.

Observed:

```text
capacity 32: insert 5->50, 37->370 (collision at slot 5), 12->120, update 5->55
lookup 5 => 55, 37 => 370, 12 => 120, 99 => None
count => 3
capacity 2: insert 1, 2, then 4 => hash.full
```

No Rust hash container, `HostOp` or new `Value` type was added.

## UTF-8 encoder fixture 59

`tests/conformance/semantics/59-utf8-encoder.mq.md` writes UTF-8 bytes into
`Memory` using `text.codepoints` and byte indexing:

* code points below `0x80` -> 1 byte;
* below `0x800` -> 2 bytes; below `0x10000` -> 3 bytes; otherwise 4 bytes;
* the fixture encodes `"Aé中𝄞"` to
  `[65, 195, 169, 228, 184, 173, 240, 157, 132, 158]` with length 10.

## UTF-8 decoder fixture 60

`tests/conformance/semantics/60-utf8-decoder.mq.md` decodes UTF-8 from
`Memory` and validates every sequence:

* `utf8.truncated` when a multi-byte sequence runs past the region length;
* `utf8.invalid_continuation` when a byte outside `0x80..0xBF` appears in a
  continuation position;
* `utf8.invalid_scalar` for overlong 3-byte forms, surrogates and values above
  `0x10FFFF`;
* `utf8.invalid_lead` for `0x80..0xC1` and `0xF5..0xFF` lead bytes.

Observed:

```text
[65,195,169,228,184,173]          -> ok "Aé中"
[228,184]                         -> utf8.truncated
[228,65,173]                      -> utf8.invalid_continuation
[237,160,128]                     -> utf8.invalid_scalar
[240,157,132,158]                 -> ok "𝄞"
```

Together with fixture 59 this closes the C04 encode/decode demonstration using
only `.mq.md`; no Rust UTF-8 codec is called.

## Graph adjacency fixture 61

`tests/conformance/semantics/61-memory-graph-adjacency.mq.md` stores a directed
graph in `Memory`:

* 16-byte node: one-byte value at 0, edge-list head `u32` at 4;
* 8-byte edge: target node offset at 0, next edge offset at 4;
* edges are head-inserted, so traversal returns the most recently added edge
  first;
* nodes `甲/乙/丙` at offsets 8/24/40, edges `甲→乙`, `甲→丙`, `乙→丙`.

Observed:

```text
甲=8, 乙=24, 丙=40
邻接(甲) => [40, 24]   # 丙 then 乙 (head insertion)
邻接(乙) => [40]
邻接(丙) => []
```

No Rust graph container, `HostOp` or new `Value` type was added.

## JSON subset parser fixture 62

`tests/conformance/semantics/62-json-array-parser.mq.md` is a recursive
descent parser over `text.codepoints`:

* supports nested arrays and objects, strings with standard escapes including
  `\uXXXX`, non-negative and negative integers, `true`/`false`/`null`,
  whitespace, and returns `{ok,value,next}`;
* errors are `Result` maps: `json.unexpected_eof`, `json.invalid_array`,
  `json.invalid_object`, `json.invalid_string`, `json.invalid_number`,
  `json.invalid_literal`, `json.unexpected_token`;
* floats are explicitly out of scope.

Observed:

```text
" [1, 23, [4, 5], -6] " -> ok [1,23,[4,5],-6], next=20
"[1 2]"                 -> json.invalid_array
"[1,"                   -> json.unexpected_eof
{"a": 1, "b": [true, null, "说\"好"]}
  -> ok {a:1, b:[true,null,"说\"好"]}, next=35
"\"a\\nb\"" -> ok "a\nb", next=6
```

No Rust JSON parser, new `Value` type or `HostOp` was added.

## Bytecode VM fixture 63

`tests/conformance/semantics/63-memory-bytecode-vm.mq.md` is a small stack
bytecode machine written entirely in `.mq.md`:

* program bytes at offset 0; stack at a caller-provided `Memory` offset;
* instruction set: `1=PUSH imm8`, `2=ADD`, `3=MUL`, `5=JMP`, `6=JZ`, `7=HALT`;
* state (`pc`, `sp`, running, error) is ordinary local state; the stack and
  program live in `Memory`;
* `(5 + 7) * 3` executes to `{ok:true, value:36}`;
* a `JZ` program jumps over a `PUSH 99` and returns `{ok:true, value:42}`.

No Rust VM, opcode enum, `HostOp` or new `Value` type was added.

## Integer codecs fixture 64

`tests/conformance/semantics/64-memory-integer-codecs.mq.md` provides generic
big-endian and variable-length integer codecs over `Memory`:

* `写大端(内存,基址,值,宽度)` writes the least significant byte last, so the
  result is big-endian for widths 2, 4 and 8;
* `读大端(内存,基址,宽度)` rebuilds the integer with `value * 256 + byte`;
* `写变长`/`读变长` are LEB128 (7-bit continuation) with the consumed byte
  count.

Observed:

```text
u16 4660                  -> 4660
u32 305419896             -> 305419896
u64 1311768467463790320   -> 1311768467463790320
u64 18446744073709551615  -> 18446744073709551615
varint 123456789          -> 4 bytes, decoded 123456789, consumed 4
last two bytes of u64::MAX -> 255, 255
```

No Rust codec builtin or new `Value` type was added.

## Table sort fixture 65

`tests/conformance/semantics/65-table-sort-stdlib.mq.md` sorts a table by a
column using only CM-A04 primitives:

* `table.row` extracts rows, `table.cell` compares the sort column;
* `插入有序` builds a new row list with `list.append` and binary condition
  branches (insertion sort);
* `表(列名, 已排序)` rebuilds the result; the input table is unchanged.

Observed:

```text
原表 rows: 甲87, 乙92, 丙71
升序 rows: 丙71, 甲87, 乙92
原表: unchanged
```

No Rust table sort, `HostOp` or new `Value` type was added.

## Table join fixture 67

`tests/conformance/semantics/67-table-join-stdlib.mq.md` joins two tables on a
column using only CM-A04 primitives:

* nested `table.row` / `table.cell` loops compare the join keys;
* matches append `list.concat(left_row, right_row)` to the output rows;
* `表(list.concat(headers...), rows)` rebuilds the result; both inputs are
  unchanged.

Observed: left `甲/乙/丙`, right `乙/甲/丁` on column 0 yields the two matched
rows `甲 87 甲 B` and `乙 92 乙 A`. No Rust join or `HostOp` was added.

## Quoted CSV fixture 68

`tests/conformance/semantics/68-csv-quoted-row.mq.md` parses one CSV row with
quoted fields:

* `"..."` quotes fields, `""` is a literal quote, commas inside quotes are
  preserved;
* empty fields are preserved;
* multi-line fields and whitespace trimming are out of scope.

Observed:

```text
甲,"乙,丙","说""好" -> ["甲", "乙,丙", "说"好"]
a,,c                 -> ["a", "", "c"]
""                   -> [""]
"a""b",c             -> ["a"b", "c"]
```

No Rust CSV parser or new `Value` type was added.

## What remains open

* JSON floats, table group, and an independent `marqdo-stdlib` repository;
* CM-A06 `Resource`/`Stream` byte-transfer ABI; EOF/partial semantics are
  reserved by ADR-0024 but not implemented;
* CM-A07–CM-A11 (stdlib naming, Table storage, provider ABI, packaging,
  strict resource policy) and P3–P6;
* the RFC §10.5 frozen-CLI closure lane and Windows/macOS evidence.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-cli --test conformance_semantics
cargo run -q -p marqdo-cli -- run \
  tests/conformance/semantics/55-first-fit-free-list-allocator.mq.md --json
cargo test --workspace --locked
```
