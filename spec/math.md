# Marqdo Math v1

Status: **frozen subset (math v1)**; product baseline `3.0.0-beta.1`. The kernel computes numbers; it never proves
theorems. `math.simplify` and `math.differentiate` are **not implemented** in
this build.

Implementation: `crates/marqdo-math` (`parse`, `evaluate`, `substitute`,
`MathError`), host adapter `crates/marqdo-host/src/kernels.rs::math`, builtin
metadata `crates/marqdo-ir/src/builtins.rs`, tests
`crates/marqdo-math/tests/kernel.rs`.

## 1. From Markdown to a value

A `math` / `math_block` node is an **inert document resource**, never an
operation. The frontend attributes every formula to its innermost enclosing
heading unit (`crates/marqdo-frontend/src/resources.rs`). `ref("#heading")`
returns a `Formula` value only when that unit contains exactly one resource;
with two or more formulas the runtime raises `ref.ambiguous` and refuses to
pick one (`spec/references.md` §5, ADR-0003).

```markdown
# @knowledge 数学

## @knowledge 圆面积公式

$$
A = \pi r^2
$$

## @exec 计算圆面积

`params(r: Number)`

`面积 = math.eval(ref("#圆面积公式"), {"r": r}, target="A")`
`return 面积`
```

Nothing is evaluated by reading or rendering the document. `math.*` must be
called explicitly and the host checks the `math` capability at execution time
(default: granted in `pure`/`project`; there is no CLI flag to revoke it today).

## 2. Grammar

The parser accepts exactly this LaTeX subset (whitespace insignificant; byte
offsets are reported for parse errors):

```text
formula     = expr [ "=" expr ] ;          single '=' only, never assignment
expr        = term { ("+" | "-") term } ;
term        = unary { ("*" | "/" | "·" | "×" | implicit) unary } ;
unary       = ("+" | "-") unary | power ;
power       = primary [ "^" unary ] ;      right associative
primary     = number
            | identifier                    Unicode letter (plus digits/underscore)
            | "\pi" | "pi" | "\e" | "e"     constants
            | "(" expr ")" | "\left(" expr "\right)"
            | "{" expr "}"                  grouping
            | "\frac{" expr "}{" expr "}"    exact division
            | "\sqrt{" expr "}"             real square root
            ;
number      = digits [ "." digits ] | "." digits ;   decimal, no exponent form
```

Rules and edge cases:

* `\cdot` and `\times` are accepted as synonyms for `*`.
* Implicit multiplication between adjacent factors: `2x`, `\pi r^2`, `(a)(b)`,
  `m a`. Concatenated letters form one identifier (`ma` is the variable `ma`;
  write `m a` or `m * a` for a product). Two adjacent number literals are a
  parse error ("ambiguous").
* Unicode identifiers are supported (`面积`, `r`, `速度`). Plain multi-letter
  identifiers are variables, but names of common elementary functions (`sin`,
  `cos`, `log`, `exp`, ...) are reserved and rejected as unsupported so
  `sin(x)` is not silently read as a product.
* Underscores are ordinary identifier characters: `x_1` is the variable `x_1`,
  not a subscript.
* A single top-level `=` creates an `Equation { left, right }`; it is **not**
  assignment and never writes a Marqdo binding. A second `=` is a parse error.
* `\frac{a}{b}` is exact division (`Div`); `1.5 + 2.5` is an exact rational
  computation, not binary floating point.
* `\sqrt{x}` is real-valued: a negative radicand is a `Type` error.
* Decimal literals have no exponent form (`1e3` is rejected); `2e` is
  `2 * e` (number times the constant), while `2e3` is rejected rather than
  reinterpreted.

### 2.1 Unsupported constructs

Everything outside the grammar is a structured `Unsupported` or `Parse` error:
matrices, sums/integrals, function applications (`\sin`, `\cos`, ...),
multi-letter commands, `\left[`/`\right]`, mismatched/missing braces,
scientific notation, commas, arrays, subscripts, superscript commands other
than the `^` operator, `\frac`/`\sqrt` without braced arguments, labels,
alignment characters, line breaks. There is no `simplify`, `differentiate`,
`integrate`, `matrix` or theorem/proof operation in this build.

## 3. Numeric semantics

* Exact rational arithmetic uses `num-rational::Ratio` over the
  `num-bigint::BigInt` type that also backs `Value::Int`; integers are
  arbitrary precision.
* Decimal literals are exact rationals (`0.25` = `1/4`). `Int` bindings become
  exact integers. `Float` bindings are interpreted as the **exact binary
  rational value** of the `f64`; non-finite `Float` bindings are a `Type`
  error.
* `+ - * / ^` are exact when both operands are exact. Division keeps a
  rational, so `\frac{1}{3} * 3` is exactly `1`.
* The final value is a Marqdo `Int` exactly when the exact result is an
  integer, and a `Float` otherwise (`crates/marqdo-math/src/eval.rs`, the
  `Numeric` enum). Examples: `1.5 + 2.5` → `Int(4)`; `1 / 2` → `Float(0.5)`;
  `100 / 4` → `Int(25)`.
* `\pi`, `\e` and a `\sqrt` of a non-perfect square introduce an `f64`
  approximation. Anything derived from them is a `Float`. Precision follows
  IEEE-754 binary64 (about 15–17 significant decimal digits); approximate
  results are never presented as exact.
* `\sqrt` of a perfect square of an exact rational stays exact (`\sqrt{4}` →
  `Int(2)`); `\sqrt{1/4}` is `Float(0.5)` in this build (it becomes
  approximate via the root path, matching the documented test).
* The exponent of `^` must evaluate to an integer. Magnitude is bounded by
  `MAX_EXPONENT`. Negative integer exponents are exact reciprocals (the result
  is therefore a `Float` unless it is integral). `0 ^ 0` is a `Type` error.
* Exact intermediate/result numerators and denominators are bounded by
  `MAX_RESULT_BITS`; exceeding the bound returns a structured `Unsupported`
  error rather than hanging or exhausting memory.
* Division by zero is a `Type` error inside the kernel. The host maps all
  `MathErrorKind::Type` to `runtime.type`; it does **not** produce
  the interpreter's `runtime.division_by_zero` code for a formula. This is a
  known divergence from the module doc comment in
  `crates/marqdo-math/src/lib.rs` and should be fixed or documented in an ADR.
* Constants are reserved: a binding named `pi` or `e` never overrides the
  constant, and they cannot be solve targets (use `E`, `x`, `r`, ...).

### 3.1 Bounds

| Constant | Value | Location |
|---|---|---|
| `MAX_SOURCE_BYTES` | 1,000,000 | `crates/marqdo-math/src/parse.rs` |
| `MAX_PARSE_DEPTH` | 256 | parse.rs |
| `MAX_AST_NODES` | 512 | parse.rs |
| `MAX_EXPONENT` | 10,000 | `crates/marqdo-math/src/eval.rs` |
| `MAX_RESULT_BITS` | 1,000,000 | eval.rs |

The AST-node bound also bounds later recursion for long operator chains, so
hostile input cannot overflow the stack during evaluation, rendering or
substitution. No code path panics on untrusted LaTeX; all failures are
`MathError`.

## 4. Equations and solving a target

`FormulaKind` is `Expression(expr)` or `Equation { left, right }`.

`evaluate(formula, bindings, target)`:

* for a plain expression, `target` must be `None`; supplying one is
  `Unsupported` ("a target variable is only meaningful for an equation");
* for an equation, `target` is required (`Unsupported` otherwise); an empty
  target is `Unsupported`;
* the target must occur **exactly once** and the equation must be **affine in
  the target**. Other variables are substituted from `bindings` first; a
  missing binding is `UndefinedVariable`.

Affine solving supported by `crates/marqdo-math/src/eval.rs`:

* `+`, `-`, multiplication by a target-free side, division where the target is
  in the numerator, and inversion of those operations in either direction:
  `2x = 4`, `4 = 2x`, `x / 2 = 3`, `3 = x / 2`, `x + 2 = 5`, `2x + 1 = 7`,
  `(x + 1) * 2 = 8`, `x = 2y + 1` (with `y` bound);
* `F = m a` solved for `F`, `m` or `a`.

Rejected as `Unsupported`: target occurs zero times or more than once, target
in a denominator, under a root, in an exponent, or in a non-affine product
(e.g. `x * x`). Equations never mutate bindings and never create Marqdo
variables.

## 5. Substitution

`substitute(formula, bindings)` replaces bound variables with exact numeric
literals and keeps unbound variables symbolic. The returned `Formula.source` is
a deterministic canonical rendering that `parse` accepts again; it is not
byte-identical to the input formatting. Host-facing `math.substitute` wraps the
result in a `Formula` value with `unit: None`, `display: false`, span of the
call site.

## 6. Host-facing surface (`math.*`)

All operations are builtins in `crates/marqdo-ir/src/builtins.rs`, dispatched
in `crates/marqdo-runtime/src/builtins_impl.rs`, and executed in the host
adapter `crates/marqdo-host/src/kernels.rs`. `math` capability is checked
before touching the kernel.

| Builtin | Parameters | Result |
|---|---|---|
| `math.parse` | `formula: Formula or Text` | `Formula` (source = input; `unit=None`, `display=false`); parse errors are `math.unsupported` |
| `math.eval` | `formula: Formula or Text`, `bindings: Map`, optional `target: Text` | `Int` or `Float`; equation requires target |
| `math.substitute` | `formula: Formula or Text`, `bindings: Map` | `Formula` with canonical source |
| `math.simplify` | `formula` | **not implemented**: runtime error `math.unsupported` ("not part of the supported math subset in this build") |
| `math.differentiate` | `formula`, `变量` | **not implemented**: `math.unsupported` |

Input typing:

* formula arguments accept a `Formula` value or `Text`; anything else is
  `runtime.type`;
* `bindings` must be a `Map` whose values are `Int` or `Float`; any other
  value type is `runtime.type` with a message naming the binding;
* `target` must be `Text` or `None`; other types are `runtime.type`;
* builtin arity/parameter-name mistakes are catchable statically
  (`call.arity`, see `spec/mapping.md` §6);
* the math kernel is a pure computation: no file, network, process or plugin
  effects, and nothing it returns is a capability.

### 6.1 Error mapping

Host mapping (`crates/marqdo-host/src/kernels.rs::math_error`):

| `MathErrorKind` | Runtime code |
|---|---|
| `Parse` | `math.unsupported` (message includes the byte offset when known) |
| `Unsupported` | `math.unsupported` |
| `UndefinedVariable` | `symbol.unknown` |
| `Type` | `runtime.type` |

## 7. What this is not

* Not a computer-algebra system: no simplification, differentiation,
  integration, matrices, series, assumptions, units or symbolic rewriting.
* Not a theorem prover: `F = m a` is an equation value, not a proof, and a
  numeric solve is not a certificate.
* Not a LaTeX renderer: rendering is the Markdown reader's job; the kernel only
  receives the raw `latex` text of a `math` node.
* Not error-free in the floating-point sense: approximation precision is
  documented, not guaranteed beyond binary64.
