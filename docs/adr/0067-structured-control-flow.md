# ADR-0067 — Multi-arm branches and nearest-loop control

> Historical public protocol sections. Current Analysis, runtime output and
> Tool contracts are specified by [ADR-0068](0068-single-current-public-contracts.md).

Status: Accepted for development. Release inclusion requires a maintainer
decision and the applicable same-SHA release gates.

Implements stage A of [RFC-CTRL-0005](https://git.cflmy.cn/cflmy/marqdo-cli/issues/7)
as an amendment to [RFC-LANG-0002](../architecture/RFC-LANG-0002-unified-semantics.md)
and the existing [mapping](../../spec/mapping.md) and
[grammar](../../spec/expression.ebnf). Follow the frontend → IR → interpreter
boundary in the [development blueprint](../DEVELOPMENT_BLUEPRINT.md#7-rust-独立解释器ir值系统与执行内核).

## Language and execution

An `else if` carrier adds an arm to the existing Branch IR. Condition
evaluation is ordered and short-circuited. A separate `else` carrier followed
by `if` keeps its nested meaning. The frontend owns pairing, heading
boundaries, and diagnostics. The Markdown profile and carrier mapping stay
the same.

A complete `break` or `continue` payload is contextual loop control. Existing
names in functions, parameters, fields, calls and expressions stay valid.
The ambiguous case is a standalone identifier read, which can be written
with parentheses. The runtime propagates a structured result to the nearest
loop, with the usual statement and iteration budgets. The loop consumes that
result within the current function. Static binding analysis only merges
reachable exits and keeps the function's initial loop-entry binding rules.

## Public projections

Branch arms fit Analysis v2. A project containing a loop transfer requires
Analysis v3, which preserves the envelope and adds the statement kinds
`break` and `continue`. The writer selects the required version from the
project. A v2 reader can keep reading existing output and rejects v3 at the
envelope. Trace v2 adds `control.break` and `control.continue` to its open
event vocabulary. Execution Events v1 adds the corresponding events, carrying
only the existing frame/module/span fields. The Trace digest binds to the
automatically selected Analysis document. Markdown, ABI and Host operation
versions are independent of this change.

Stage B List iteration requires a separate decision on snapshot and binding
semantics. Release approval and ecosystem adoption are tracked outside this
implementation.
