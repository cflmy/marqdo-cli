# ADR-0044: Explicitly granted `process.run` resource operation

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0041, ADR-0042, ADR-0043
Specs: `spec/security.md`, `spec/evaluation.md`
Evidence: `docs/evidence/2026-11-21-process-product.md`

## Context

The audit RFC lists a `std:process` product module. Spawning a process is the
most privileged OS operation in the runtime and is **not** contained by the
in-process host checks. Without an explicit contract it would silently turn
the CLI into an unrestricted command runner.

## Decision

Add a project-mode-only `resource.request` operation:

```text
resource.request("process.run", [program, args]) -> {ok:true, value:{status, stdout, stderr}}
                                                   | {ok:false, code:"process.not_found"}
                                                   | {ok:false, code:"process.timeout"}
                                                   | {ok:false, code:"process.output_too_large"}
```

* `program` is an exact Text path; `args` is a List of Text values. Wrong
  arity/types are fatal. There is no shell expansion, no glob and no implicit
  `PATH` lookup.
* A new exact `Capabilities::process_programs: Vec<String>` grant is required
  (`--allow-process PROGRAM`). Un-granted programs return
  `permission.denied` before spawning.
* The child runs with `env_clear`, stdin `null`, the project root as its
  working directory, and a 5 second wall-clock limit. On timeout it is killed
  and the call returns recoverable `process.timeout`.
* `stdout`/`stderr` are captured separately, require valid UTF-8 (otherwise
  fatal type error), and are capped at 1 MiB each; exceeding the cap is
  recoverable `process.output_too_large`.
* `pure`/`strict` refuse the operation. This does **not** make the child an OS
  sandbox: the in-process host check is cooperative and the child inherits the
  CLI user's OS privileges.
* `stdlib/process.mq.md` is a thin `.mq.md` facade; no Rust builtin or
  `HostOp` is added.

## Consequences

* Process execution is opt-in per exact program path and remains project-mode
  only.
* The operation is not a sandbox. A future strict-mode process provider needs
  OS isolation evidence and a separate ADR.
* `--allow-process` is rejected by strict preflight, like the other external
  grants.
