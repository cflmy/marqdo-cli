# ADR-0043: Explicitly granted `env.get` resource operation

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041, ADR-0042
Specs: `spec/security.md`, `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-env-product.md`

## Context

A product `std:env` module needs to read process environment variables. The
environment is an ambient OS side channel that can contain secrets and
credentials. Reading it through a wildcard path grant or by default would
silently widen the capability surface; `env.list` would expose every variable
without naming it.

## Decision

Add one operation to `resource.request`:

```text
resource.request("env.get", [name]) -> {ok:true, value:Text}
                                     | {ok:false, code:"env.not_found"}
                                     | {ok:false, code:"env.not_unicode"}
```

* Arguments: exactly one `Text` variable name. Non-Text or wrong arity is a
  fatal type/arity error.
* Grants: a new `Capabilities::env: Vec<String>` with **exact name matching**
  (no glob). `--allow-env NAME` adds one grant; un-granted names return
  `permission.denied` before the OS is read.
* The check happens before `std::env::var`. Project mode only; `pure` and
  `strict` refuse. No default grant.
* Missing variables and non-UTF-8 variables are recoverable
  `env.not_found`/`env.not_unicode`, not fatal errors.
* There is no `env.list`, `env.set`, process spawning or `std::env::set_var`
  in this decision. A future `env.list` would need a separate ADR because it
  changes the disclosure model.
* `stdlib/env.mq.md` is a thin `.mq.md` facade: `读取` returns the Result and
  `存在` maps `env.not_found` to `{ok:true, value:false}`.

## Consequences

* The environment is readable only by exact, operator-supplied names in
  project mode.
* The CLI gains `--allow-env NAME`; strict mode rejects it with exit 2.
* `std:process` remains unsupported in this decision; process execution needs
  its own capability and OS-sandbox story.
