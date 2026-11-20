# Marqdo Value Wire Format v1

Status: **frozen for M0.5**. Changes require an ADR, a `wire_version` bump and
cross-ABI compatibility tests.

Canonical implementation: `crates/marqdo-value/src/wire.rs`
(`to_wire`, `from_wire`, `from_wire_with_limits`, `WireLimits`, `WireError`).
The plugin envelope that *carries* these values is defined in
`crates/marqdo-plugin-abi/src/wire.rs` and documented in
[`plugin-abi.md`](plugin-abi.md); the foreign-kernel stdin envelope is
documented in [`foreign.md`](foreign.md) and implemented in
`crates/marqdo-foreign/src/lib.rs`.

## 1. Why a typed wire format

JSON has one numeric type. Marqdo has arbitrary-precision `Int`, IEEE-754
`Float` (including `NaN` and both infinities) and 13 distinct runtime value
types (`crates/marqdo-value/src/value.rs`). Encoding an `Int` as a JSON number
would silently lose digits above 2^53 and would make `Int(1)` and
`Float(1.0)` indistinguishable after a round trip. The wire format is
therefore **fully type-tagged** and is not plain JSON.

The rules that follow from this:

1. Every value is a JSON **object** with a mandatory `"type"` string.
2. **Integers never travel as JSON numbers.** `Int` is
   `{"type":"int","decimal":"<base-10 string>"}`, which preserves every digit,
   sign and leading-zero-free canonical form.
3. `Float` uses a JSON number only for finite values; `NaN`, `Infinity` and
   `-Infinity` use an explicit `"special"` tag.
4. Unknown tags, missing fields, wrong JSON types and invalid payloads are
   structured decode errors, never silent coercions.

JSON numbers still appear in two non-value positions: `plugin_handle.id` (a
host generation counter) and `span.start` / `span.end` (byte offsets). Neither
is a user arithmetic value.

## 2. Version and entry points

* `WIRE_VERSION: u32 = 1` (`marqdo_value::WIRE_VERSION`).
* `to_wire(&Value) -> serde_json::Value` encodes.
* `from_wire(&Json) -> Result<Value, WireError>` decodes with
  `WireLimits::default()`.
* `from_wire_with_limits(&Json, WireLimits) -> Result<Value, WireError>`
  decodes with explicit limits.
* Decoding is recursive and never panics on untrusted input; every failure is
  a `WireError`:
  * `UnknownType(String)` — missing/non-string/unknown `"type"`;
  * `MissingField(&'static str)` — required field absent;
  * `Invalid(String)` — wrong JSON type, invalid decimal, bad table shape,
    unknown float `special`, non-object value;
  * `LimitExceeded(&'static str)` — nesting depth, list/map length, string or
    map-key length.

`WireError` implements `Display` but has no stable numeric code of its own.
Boundary crates wrap it in their own stable codes: the plugin layer uses
`invalid_argument_wire` / `invalid_result_wire`
(`crates/marqdo-plugin-abi/src/wire.rs`), and the foreign layer maps it to
`Protocol` (`crates/marqdo-foreign/src/lib.rs`).

## 3. Value encodings

The table is normative and mirrors `encode`/`decode` in
`crates/marqdo-value/src/wire.rs`.

| Marqdo type | Wire JSON | Notes |
|---|---|---|
| `Unit` | `{"type":"unit"}` | No result; distinct from `None`. |
| `None` | `{"type":"none"}` | Explicit empty value. |
| `Bool` | `{"type":"bool","value":true}` | `value` must be a JSON boolean. |
| `Int` | `{"type":"int","decimal":"-123..."}` | Decimal string; decoded with `num_bigint::BigInt::from_str`. No `+`, no separators, no hex. `-0` decodes to `0` because `BigInt` has no signed zero. |
| `Float` (finite) | `{"type":"float","value":1.5}` | JSON number decoded with `as_f64()`. `-0.0` round-trips as a JSON negative-zero number and decodes to `Float(-0.0)`. |
| `Float` (`NaN`) | `{"type":"float","special":"nan"}` | Never a JSON number. |
| `Float` (`+∞`) | `{"type":"float","special":"infinity"}` | |
| `Float` (`−∞`) | `{"type":"float","special":"-infinity"}` | |
| `Text` | `{"type":"text","value":"..."}` | UTF-8 JSON string. |
| `List` | `{"type":"list","items":[<value>, ...]}` | Order preserved. |
| `Map` | `{"type":"map","entries":{"key":<value>, ...}}` | Decoding rebuilds a `BTreeMap`, so key order is canonical (sorted) after a round trip. Duplicate keys cannot occur in a JSON object. |
| `Table` | `{"type":"table","header":["a","b"],"rows":[[<value>,<value>], ...]}` | Header cells must be JSON strings; every row must have exactly `header.len()` cells or decoding fails. |
| `Formula` | `{"type":"formula","latex":"...","display":false}` plus optional `"unit"` (string) and `"span":{"start":N,"end":M}` | `display` defaults to `false` on decode when absent. The formula is inert data; this encoding never evaluates it. |
| `Code` | `{"type":"code","language":"python","source":"...","content_hash":"<sha256 hex>"}` plus optional `"unit"` and `"span"` | `language` and `content_hash` default to `""` on decode when absent; `source` is required. |
| `FunctionRef` | `{"type":"function_ref","module":"index.mq.md","name":"main"}` | Both fields required strings. |
| `PluginHandle` | `{"type":"plugin_handle","plugin":"demo.solver","export":"solver.add","id":1}` | `id` must be a non-negative JSON integer (`as_u64`). |

Optional fields (`unit`, `span`, `display`, `language`, `content_hash`) are
emitted only when present on the value. Encoders always emit `display` for
`Formula` and `language`/`content_hash`/`source` for `Code`.

### 3.1 Round-trip guarantees

For the values covered by `wire::tests::scalar_roundtrip`,
`from_wire(to_wire(v)) == v`. Exact guarantees:

* all `Int` digits survive (`big_integers_keep_all_digits`);
* `NaN`/`±Infinity` survive with their sign/tag (`special_floats_are_tagged`);
* `Unit` and `None` never collapse into each other;
* `Map` iteration order after decoding is sorted, which is the canonical order
  used by `Value::display`/`repr`.

Finite `Float` values are encoded as JSON numbers. The workspace enables
`serde_json`'s `float_roundtrip` feature, so a finite binary64 value survives an
encode/decode cycle exactly (verified by the Python `math.pi * 9` semantic
fixture: `28.274333882308138` stays `28.274333882308138`). `NaN`/infinities are
tagged and exact. Use `Text` when a decimal string must be exchanged verbatim
rather than as a binary64 float.

## 4. Limits

`WireLimits::default()` (`crates/marqdo-value/src/wire.rs`):

| Limit | Default | Applies to |
|---|---|---|
| `max_depth` | 64 | Nesting of *values*: the top-level object is depth 0, every child decoded through `list.items`, `map.entries`, `table.rows` or `table.header` cells is depth+1. A value at depth > 64 is rejected with `LimitExceeded("nesting depth")`. |
| `max_items` | 1,000,000 | `list.items` length and `map.entries` key count. |
| `max_string_bytes` | 16 MiB | `text.value` byte length and map key byte length. |

### 4.1 Known gaps in the current decoder

The following are **not** bounded by `WireLimits` in
`crates/marqdo-value/src/wire.rs` today and must be treated as open items
(they are also bounded indirectly at the ABI layer by envelope size limits):

* `table.header` length, `table.rows` length and the number of columns are not
  checked against `max_items` (only row/header width equality is checked);
* table header cell strings are not checked against `max_string_bytes`;
* `formula.latex`, `code.source`, `function_ref.module`/`name`,
  `plugin_handle.plugin`/`export` are not checked against
  `max_string_bytes`;
* a `plugin_handle` decoded from wire data is a structurally reconstructed
  `Value::PluginHandle`; it is **not** an authorization token. The host owns a
  `PluginHandleTable` with owner/run/generation scope (ADR-0014), and plugin or
  foreign output containing any handle is rejected with
  `plugin_handle.untrusted`. Host code must therefore treat a plugin handle as opaque *intent* and
  re-authorize any operation, not as proof of prior authorization. The header
  comment "cannot be forged from wire data" is aspirational for this point and
  should be fixed or reworded in a future ADR.
* JSON itself may contain duplicate object keys; `serde_json` keeps the last
  one, so `map.entries` with duplicate keys is not rejected as an error.

## 5. Where the format is used

| Boundary | Envelope | Value positions |
|---|---|---|
| Plugin ABI v1 request/response | `{"wire_version":1,"kind":"request"/"response",...}` (`crates/marqdo-plugin-abi/src/wire.rs`) | `request.args` is an object of name → typed value; `response.value` is one typed value. |
| Plugin host capability request/reply | opaque bytes, shape per capability | defined by the capability, not by this file. |
| Foreign kernel stdin | `{"wire_version":1,"kind":"request","input":<typed value or null>}` (`crates/marqdo-foreign/src/lib.rs`) | `input` is a `to_wire(value)` result, or JSON `null` when absent. |
| Foreign typed return (`foreign.call`) | stdout must be exactly one typed value | decoded with `from_wire` and default limits. |

The `--json` output of `run`/`call` embeds `"value": to_wire(outcome.value)`
(`crates/cli/src/commands.rs`), so CLI JSON consumers see the same typed
representation.

## 6. Compatibility rules

* `wire_version` is part of the frozen contract. A decoder rejects a different
  version rather than guessing.
* Adding a new `"type"` tag requires a spec change and an ADR; removing or
  renaming a tag is a breaking change.
* The plugin envelope independently carries `wire_version`; do not assume the
  C ABI major version and `wire_version` are the same number
  (see `spec/plugin-abi.md` §2).
* All payloads are UTF-8 JSON. Endianness is irrelevant for the JSON layer,
  but the C ABI structs that carry these bytes are target-dependent.
