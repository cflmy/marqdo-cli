# ADR-0004: Strict mode refuses to run without OS isolation

Status: **Accepted**
Date: 2026-11-20

## Context

Blueprint §9 defines three execution modes and one non-negotiable rule:

> `strict`: untrusted documents/code/plugins — independent restricted process,
> OS-level resource and syscall/file/network restrictions; if unavailable,
> **refuse to run**.

A cooperative in-process capability check cannot contain hostile native code
or an unrestricted child process. If `strict` silently degraded to `project`,
users would believe they had isolation they did not have. That is a security
bug, not a missing feature.

M0.5 implements capability checks, budgets and process supervision but **no OS
isolation adapter** (no Linux namespaces/seccomp/cgroups, no Windows Job
Object/appcontainer, no macOS sandbox), and the isolated plugin worker
transport is not implemented either.

## Decision

1. `crates/marqdo-sandbox` is honest about platform support:
   * `PlatformCapabilities::detect()` always reports
     `StrictIsolation::Unavailable { reason }` in this build, with a
     platform-specific reason (Linux: "no OS isolation adapter
     (namespaces/seccomp/cgroups) is implemented in this build"; macOS/Windows/
     other targets have analogous messages);
   * `worker_processes: false`.
2. `marqdo_sandbox::enforce(mode, platform)`:
   * `pure` and `project` → `Ok(())`;
   * `strict` → `Err(SandboxError::StrictUnavailable { reason })` unless
     `platform.strict.is_available()`.
3. `run`/`call` call `enforce` **before** constructing the host, loading
   plugins or executing anything. On failure they print
   "refusing to run in strict mode: ..." (or a JSON `{"success":false,
   "error":...}` envelope) and exit 1.
4. There is **no fallback**: the CLI never downgrades a requested strict run to
   project mode. `SandboxMode::parse("strict")` succeeds (the mode name is
   valid); only execution is refused.
5. `inspect --platform` exposes the detection report so users can see why:
   `{"os":"linux","arch":"x86_64","strict":{"state":"unavailable","reason":...},
   "worker_processes":false}`.
6. The rest of the system is labelled accordingly: `project` mode is
   cooperative, in-process, and is not a sandbox; foreign kernels are
   process-isolated but not confined; native plugins run in-process and require
   explicit trust (ADR-0006/security spec).

## Alternatives

1. **Best-effort strict** (run with the same checks as project). Rejected: it
   directly violates the blueprint's "cannot provide → refuse" rule and hides a
   missing security boundary.
2. **Warn and continue.** Rejected for the same reason; a warning is not a
   sandbox, and CI/agents may not read warnings.
3. **Implement a Linux namespaces/seccomp adapter now.** Out of scope for
   M0.5: it needs platform-specific code, integration tests, resource limits,
   CI on multiple kernels, and a way to supervise the worker protocol. This is
   P4 work and belongs in its own ADR when implemented.
4. **Use Wasm/WASI for strict.** Attractive for plugins later, but it does not
   sandbox arbitrary Python/Node processes or native `.so` files, and the
   runtime is not implemented. Rejected for now.
5. **Claim strict via the process isolation of foreign kernels.** Rejected:
   a child process inherits the user's privileges and filesystem/network
   access; changing cwd and clearing env is hygiene, not confinement.

## Markdown AST shape

No language or AST change. A strict run is rejected before any document is
executed (after static loading/diagnostics); the CLI contract is:

```text
$ marqdo run index.mq.md --mode strict
error: refusing to run in strict mode: no OS isolation adapter
       (namespaces/seccomp/cgroups) is implemented in this build. Strict mode
       requires verified OS-level isolation and never falls back to an
       unrestricted process.
$ echo $?
1
```

(Exact wording comes from
`crates/marqdo-sandbox/src/lib.rs::SandboxError::display`.)

## Compatibility

* Accepting `--mode strict` as a valid *name* is part of the CLI contract; the
  refusal is a documented behavior, not a parse error. A future release that
  implements isolation flips the detection report and starts executing strict
  runs — that is the intended upgrade path.
* Exit code 1 (runtime/security refusal) is stable; usage errors are 2.
* `PlatformCapabilities` JSON shape (`state: available|unavailable`) is
  versioned by documentation and should not change without an ADR.
* No package/ABI/wire impact.

## Security

* The guarantee "requested strict → refused rather than downgraded" is
  enforced in code (`enforce`), not documentation.
* Users cannot accidentally believe untrusted docs are sandboxed by this
  build; the platform report is the evidence.
* Project mode remains explicitly non-sandboxed. Native plugins (in-process)
  and foreign child processes are recorded as trust decisions, not isolation.
* Secrets: strict refusal happens before plugin loading or foreign spawning,
  so no untrusted code is touched by a refused command.

## Testing

* `crates/marqdo-sandbox/src/lib.rs::tests::
  strict_refuses_when_isolation_is_unavailable` asserts the refusal and that
  `pure`/`project` still succeed.
* `mode_names_round_trip` covers parsing/printing.
* CLI behavior (`--mode strict` exit 1, message) is covered by manual/CLI
  smoke checks; a CLI integration test should be added with the conformance
  harness.
* `inspect --platform` JSON is exercised manually and in the CLI help/example
  documentation; it should get a golden test in the fixture workstream.

## Migration & rollout

When an OS isolation adapter lands:

1. implement a `StrictIsolation::Available { mechanism }` report and a real
   supervisor/worker;
2. add integration tests for filesystem/network/process restrictions and
   resource limits per platform, plus the platform capability matrix;
3. update `spec/security.md`, this ADR (or supersede it), `docs/STATUS.md` and
   the CLI docs;
4. only then change `PlatformCapabilities::detect`; do not add a flag that
   bypasses the refusal before that work is done.
