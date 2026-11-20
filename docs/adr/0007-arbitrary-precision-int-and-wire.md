# ADR-0007: Arbitrary-precision `Int` and the typed wire encoding (S-08)

Status: **Accepted**
Date: 2026-11-20

## Context

The blueprint (§3.1 and unresolved issue **S-08**) requires a numeric model
that does not silently lose precision at language, JSON/ABI and math
boundaries. It recommends arbitrary-precision integers for `Int` and type tags
plus string forms for large integers on the wire.

Three independent places could lose information:

1. **Language arithmetic.** A fixed-width `i64` would make `Int` silently
   overflow or wrap. Factorials, powers and scientific constants routinely
   exceed 64 bits. The expression grammar already has arbitrary-length integer
   literals (`spec/expression.ebnf`).
2. **JSON / wire.** JSON has one number type (IEEE-754 double in most parsers).
   Encoding `12345678901234567890` as a JSON number loses digits in
   JavaScript/Go consumers; encoding `1` as a number makes `Int` and `Float`
   indistinguishable.
3. **Math kernel.** The exact-rational engine needs an integer type that is
   the *same* type as `Value::Int`, or every formula/binding crossing the host
   boundary needs a lossy conversion.

The chosen dependency set (`Cargo.toml`) is:

```toml
num-bigint = { version = "0.4", features = ["serde"] }
num-rational = { version = "0.4", features = ["serde"] }
num-traits = "0.2"
```

## Decision

### 1. `Int` is arbitrary precision (`num-bigint::BigInt`)

* `Value::Int(BigInt)` (`crates/marqdo-value/src/value.rs`) is the only integer
  representation in the runtime and the expression AST
  (`crates/marqdo-expression/src/ast.rs`).
* Expression integer literals are arbitrary-precision decimal; there is no
  fixed-width parsing step.
* Arithmetic (`crates/marqdo-runtime/src/value_ops.rs`):
  * `+ - *` on two `Int` are exact `BigInt` operations;
  * `/` **always** produces `Float` (exact ratio converted to binary64), so
    there is no integer division surprise;
  * `//` and `%` require `Int` operands; floor division is mathematical floor
    and `a % b == a - b * (a // b)`, with the sign following the divisor;
  * mixed `Int`/`Float` arithmetic produces `Float`; converting an `Int`
    larger than 2^53 may lose precision (documented, not silently exported as
    an `Int`);
  * `Int`/`Float` comparisons are **exact**: the float is treated as the exact
    binary rational it is (`BigRational::from_float`), so
    `9007199254740993 > 9007199254740992.0` is `true`;
  * `**` on `Int` is exact exponentiation, with a negative exponent rejected
    as a type error and guards `MAX_EXACT_EXPONENT = 100_000` and
    `MAX_POWER_BITS = 1_000_000` (`resource.exhausted`).
* There is no truthiness and no implicit string↔number conversion; `整数`/`int`
  performs explicit conversions and rejects non-integral floats.

### 2. `num-bigint` 0.4 is pinned to match `num-rational` 0.4

`num-rational`'s generic `Ratio<T>` is instantiated as
`Ratio<num_bigint::BigInt>` in `crates/marqdo-math/src/ast.rs`
(`pub type Rational = Ratio<BigInt>`). The two crates must resolve to the same
`num-bigint` version, otherwise `BigInt` values from the value model cannot be
passed to `Ratio` without conversion (or a duplicate type). The workspace pins
both at the `0.4` line and the lockfile resolves one `num-bigint`. Upgrading
either crate requires checking that this shared-type invariant still holds and
running the math + runtime test suites.

### 3. The wire encoding never sends an integer as a JSON number

Canonical encoding (`crates/marqdo-value/src/wire.rs`; full contract in
`spec/value-wire.md`):

```json
{"type":"int","decimal":"12345678901234567890"}
```

* Every value is a JSON object with a `"type"` tag.
* `Int` uses a base-10 string. All digits, sign and magnitude survive any JSON
  consumer including JavaScript.
* `Float` uses a JSON number only when finite; `NaN`, `Infinity`,
  `-Infinity` use explicit `"special"` tags.
* JSON numbers appear only in non-value positions (`span.start/end`,
  `plugin_handle.id`).
* Decoding is validated: unknown tags, missing fields, invalid decimals, wrong
  types and limits are `WireError`s, never coercions.
* The same encoding is used for plugin ABI request/response values, foreign
  kernel stdin `input`, foreign typed return values and `run --json` values.

### 4. Math results preserve exactness where possible

The math kernel keeps exact rationals and returns `Value::Int` exactly when the
exact result is an integer, otherwise `Value::Float`. Decimal literals are
exact rationals (`0.25` = `1/4`), and `\frac{1}{3} * 3` is exactly `1`. Constants
(`\pi`, `\e`) and non-perfect square roots introduce binary64 approximations and
the result is a `Float` (`spec/math.md` §3).

## Alternatives

1. **`i64`/`u64` integers with overflow errors.** Rejected: common programs
   (factorials, powers) would need explicit bignum workarounds; the language
   would inherit fixed-width semantics not implied by its arithmetic.
2. **`i128` integers.** Same problem, just a bigger ceiling; and the wire
   precision problem remains for values beyond 2^53.
3. **Encode integers as JSON numbers and hope consumers use bignum JSON.**
   Rejected: the JSON standard/ecosystem does not guarantee it; JavaScript's
   `JSON.parse` loses digits; this is the exact S-08 failure.
4. **Encode every number as a string without type tags.** Rejected: loses the
   `Int`/`Float` distinction and makes all consumers parse strings; type tags
   are explicit and cheap.
5. **Use `f64` for all numbers.** Rejected: contradicts exact `Int`
   semantics, the math kernel's exact rational requirement, and the
   no-silent-precision-loss rule.
6. **Different integer crates in value vs math.** Rejected: forces lossy or
   string conversions at every kernel boundary and duplicates tests.

## Markdown AST shape

No AST change. Numeric literals live inside executable payloads only (code
spans/plain text of `strong`/`emph`/conditions). A `math` node is inert LaTeX;
its numeric interpretation is the math kernel's job. The JSON inspection view
of IR/AST contains spans as JSON numbers, not user numeric values.

## Compatibility

* `Int` semantics (arbitrary precision, `/` → `Float`, exact comparisons,
  `//`/`%` sign rules) are frozen language behavior; changing them requires an
  ADR and conformance fixtures.
* `wire_version = 1` with typed tags and decimal-string integers is frozen.
  Changing the integer representation is a wire-breaking change and needs a
  new `wire_version` plus plugin ABI considerations.
* The dependency pin may move within `0.4` for security fixes, provided
  `num-bigint` remains a single resolved version shared with `num-rational`.
* Package/lock formats are unaffected.

## Security

* Untrusted wire data cannot trigger huge allocations through the integer
  parser without also counting against value depth/string limits: a decimal
  string is bounded by `max_string_bytes` (16 MiB default) and `BigInt` parsing
  is linear. The current decoder does not separately cap the *digit count* of
  an `Int`; the envelope/string caps are the effective bound (open item).
* Arithmetic can grow memory; the runtime bounds it with fuel and the
  `Int` power guards (`MAX_EXACT_EXPONENT`, `MAX_POWER_BITS`) and the math
  kernel bounds exact numerator/denominator bits (`MAX_RESULT_BITS`).
* Exact `Int`/`Float` comparison prevents a class of "looks equal but is not"
  authorization or verification bugs.
* No new capabilities; numeric parsing is pure.

## Testing

* `crates/marqdo-value/src/value.rs`: `big_integers_are_arbitrary_precision`,
  `float_to_bigint_requires_integral_value`, display/format tests.
* `crates/marqdo-value/src/wire.rs`: `big_integers_keep_all_digits`,
  `special_floats_are_tagged`, scalar round trips, untrusted-input limits.
* `crates/marqdo-runtime/src/value_ops.rs`: exact integer arithmetic,
  `/`→Float, floor/mod sign rules, exact mixed comparisons, power guards,
  division-by-zero.
* `crates/marqdo-math/tests/kernel.rs`: exact big integer arithmetic, exact
  rational division, decimal literals as rationals, `pi`/`sqrt` approximating
  to `Float`, `Int` vs `Float` result typing.
* `crates/marqdo-plugin-abi/src/wire.rs`: request round trips preserving tags
  and refusing coercible-looking values beyond `i64`.
* `crates/marqdo-foreign/tests/kernels.rs`: typed foreign return values decode
  exactly.

## Migration & rollout

No migration: the implementation has used `BigInt` and the typed wire format
since M0.5. For future dependency upgrades: run `cargo test --workspace`,
verify one resolved `num-bigint` (`cargo tree -i num-bigint`), and re-run wire
compatibility tests. For any change to the wire integer encoding, bump
`wire_version`, update `spec/value-wire.md`, the plugin ABI documentation and
the foreign protocol in one coordinated change.
