# Architecture Decision Records (ADRs)

These files record decisions **at the date they were made**. They are not a
description of current language behavior, implementation state or current
architecture; read them as decision history.

- **Status is not a current-language claim.** A status such as "Accepted" means
  the decision was accepted then, not that its syntax or behavior is still
  active. Superseded ADRs keep their original decision text and date; they are
  not rewritten.
- **Syntax-role decisions are superseded by
  [ADR-0052](0052-unified-markdown-semantics.md).** The earlier mapping —
  `Strong` executes, `Emph` returns, ordered list is a branch, bullet list is a
  loop, block quote is a call, plus the associated `markdown.inert_executable` /
  `markdown.invalid_construct` role diagnostics — is retired. The current
  language contract is
  [RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md), with its
  normative summaries [`spec/mapping.md`](../../spec/mapping.md) and
  [`spec/expression.ebnf`](../../spec/expression.ebnf): `@exec`/`@knowledge`
  heading modes, `Strong`/`Emph`/inline `Code` as interchangeable
  one-statement payload carriers, and `if`/`else`/`while`/`end`/`return`/
  `params` keywords. The implementation is green at `ffd1cc9`; see
  [`docs/STATUS.md`](../STATUS.md).
- **Infrastructure ADRs** (crates, ABI, capabilities, CLI/JSON contract,
  packaging, security isolation) remain current unless a later ADR supersedes
  them. ADRs with a historical banner keep the banner scoped to the superseded
  part.
- This index is not a spec. For current architecture see
  [`docs/architecture/CURRENT.md`](../architecture/CURRENT.md); for executed
  status see [`docs/STATUS.md`](../STATUS.md).

Files: `0001`–`0050`, plus `0051-byte-exact-http-framing.md` (HTTP framing) and
`0052-unified-markdown-semantics.md` (current language mapping).
