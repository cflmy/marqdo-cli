# ADR-0050: Host-backed extended regular expressions

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041
Specs: `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-regex-extension.md`

## Context

The pure `.mq.md` regex subset covers literals, `.`, quantifiers, anchors and
character classes, but not groups, alternation or captures. Implementing a
full backtracking engine in `.mq.md` would be a large, hard-to-audit
duplicate. The host already has a safe, well-tested `regex` crate.

## Decision

Add project-mode operations:

```text
resource.request("regex.match",    [pattern, text]) -> {ok:true, value:Bool}
resource.request("regex.captures", [pattern, text]) -> {ok:true, value:{matched:Bool, groups:List<Text|None>}}
```

* Both require two Text arguments; invalid patterns are recoverable
  `regex.invalid`.
* `captures` includes group 0 (the whole match); unmatched optional groups are
  `None`.
* No new capability grant: regex compilation/matching is deterministic CPU
  work and does not disclose external state. All `resource.request` operations
  remain project-mode only, so `pure`/`strict` refuse them.
* `stdlib/regex.mq.md` exposes `扩展匹配`, `扩展搜索` and `捕获`; the existing
  pure `匹配`/`搜索` subset stays unchanged for pure mode.
* The `regex` crate is Rust-only, with no OS dependency. No new builtin,
  `HostOp` or parser node is added.

## Consequences

* Groups, alternation, `\d` classes and captures are available in project mode.
* Pure mode still has only the documented subset; this is an explicit
  mode-dependent capability, not a silent downgrade.
* Look-around, backreferences and regex captures in pure `.mq.md` remain open.
