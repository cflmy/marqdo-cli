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

Files: `0001`–`0050`, plus:

* `0051-byte-exact-http-framing.md` — HTTP framing;
* `0052-unified-markdown-semantics.md` — current language mapping;
* `0053-strict-child-inspection-and-json-failure-contract.md` — strict CI/JSON
  failure contract;
* `0054-callable-binding-single-source.md` — one registry/binding pass and
  runtime dispatch by identity;
* `0055-resource-provider-cli-v2.md` — `provider.json` v2 through the normal
  CLI, worker trust boundary;
* `0056-extension-selection-locking.md` — `marqdo.yaml.extensions` and lock v4;
* `0057-capability-handler-registration.md` — declared capability vs installed
  handler vs audit;
* `0058-stdlib-public-alias.md` — verified stdlib `public_aliases` replace
  print magic;
* `0059-runtime-observability-and-execution-events.md` — typed host
  observation facade, Execution Events v1 JSONL boundary, frame identity and
  selected-value policy;
* `0060-heading-modes-and-extensible-annotations.md` — `@exec`/`@knowledge`
  are the closed Core Modes; metadata-only Heading Annotations v1 is Proposed
  (target 3.1), Analysis v2 unchanged;
* `0061-detach-domain-math-kernel-from-core.md` — remove `math.*` builtins,
  `HostOp` variants and `marqdo-math`; pure package + optional ABI backend take
  over; `Formula`/`math.compute` boundaries;
* `0062-resource-operation-ownership-and-core-kinds.md` — single
  `CORE_RESOURCE_KINDS` registry, two `resource_request` operation classes and
  the reviewed architecture gate;
* `0063-foreign-kernel-transition-fa.md` — retain the permission-gated one-shot
  foreign primitive as F-A; F-B removal requires a versioned generic
  process/worker capability.
* [0064](0064-stable-core-extension-boundary.md) — core surface review and
  frozen external conformance;
* [0065](0065-resource-control-vnext.md) — proposed manifest-driven stateful
  operations and child resources;
* [0066](0066-structured-tasks-v1.md) — proposed task isolation, bounded
  communication and cancellation;
* [0067](0067-structured-control-flow.md) — multi-arm branches and nearest-loop transfers.
* [0068](0068-single-current-public-contracts.md) — one current contract per
  pre-stable public boundary, replaced in place;
* [0069](0069-public-analysis-boundary.md) — one public Analysis projection for
  independent document, lint and knowledge consumers.
* [0071](0071-engine-host-contract-boundary.md) — Engine / Host Contract /
  Native Host / CLI responsibilities, dependency rules, plugin ABI contract split
  and the keep-in-workspace repository decision.

* [0072](0072-sdk-independent-repository.md) — independent public
  `cflmy/marqdo-plugin-sdk` repository pinned to one CLI contract `rev`.
