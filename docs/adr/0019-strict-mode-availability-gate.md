# ADR-0019: Strict-mode availability gate

Status: **Accepted; Linux availability superseded by ADR-0021**
Date: 2026-11-20
Relates to: ADR-0004 (strict mode refuses without isolation), blueprint §9

## Context

`strict` is the only mode that may claim isolation for untrusted documents.
The project must never advertise strict execution merely because an isolated
worker process or a cooperative path check exists. At the time of this ADR no
platform adapter has passed the required OS-level escape and resource tests.

The environment used for this stabilization work cannot create namespaces
(`unshare`/`bwrap` return "Operation not permitted"), so a real Linux adapter
cannot be built *and verified* here. Landlock is present but covers filesystem
rules only; it does not by itself satisfy the network/process/resource and
child-tree requirements in `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`
§13.

## Superseding note (2026-11-20)

The fail-closed principle and the per-platform, test-gated availability rule in
this ADR remain in force. A concrete Linux adapter has since passed the
security matrix in a disposable Ubuntu VM and is specified by **ADR-0021**:
strict becomes `Available` only when the Bubblewrap probe succeeds at runtime.
This ADR remains authoritative for every other platform and for the rule that
availability must never be assumed.

## Decision

### 1. Strict stays unavailable until the gate passes

`PlatformCapabilities::detect` reports `StrictIsolation::Unavailable` with a
platform-specific reason on every platform. `enforce` refuses strict before any
host or plugin is constructed. There is no fallback to `project`, no "warn and
continue", and no opt-in flag that silently disables the refusal.

This state is deliberate design output, not an incomplete branch: G-B does not
depend on it, and every unsupported platform must fail closed.

### 2. Availability is a per-platform, test-gated decision

A platform may report `Available { mechanism }` only when all of the following
hold on that platform and are covered by CI on a dedicated security lane:

* filesystem visibility restricted to the project and explicit grants;
  escapes via `..`, absolute paths, symlinks and TOCTOU are denied;
* network socket creation/connection denied unless explicitly granted;
* process creation denied unless explicitly granted; the child tree is reaped
  and leaves no orphans;
* resource limits for CPU, memory, file descriptors, process count, output and
  wall clock are enforced;
* plugin workers and foreign kernels inherit the same restrictions, or strict
  refuses to load them;
* the adapter is probed at runtime; failure to obtain the mechanisms leaves
  strict `Unavailable` with the real reason;
* `--trust-native-plugin` and in-process native loading are rejected in strict,
  since C ABI code shares the host address space;
* a probe/escape test suite demonstrates denial of reading `/etc/...`, writing
  outside the project, connecting to the network and spawning a process.

Until then, `strict` must refuse even when other flags (`--allow-foreign`,
`--plugin`, `--trust-native-plugin`, read/write grants) are present.

### 3. Capability execution is not isolation

The broker executing `fs.read` for native and worker plugins (ADR-0018) is a
cooperative API contract with audit; it does not prevent a plugin from issuing
direct syscalls. It is not evidence for strict availability.

### 4. Observability

`marqdo inspect --platform --json` exposes the state and reason. The CLI test
suite asserts that the state is `unavailable` and that a strict run exits 1 with
a refusal message. Changing this contract requires this ADR to be superseded
together with the security test suite.

## Consequences

* No G-C claim is made. Documentation and CI must not describe worker
  processes, Landlock-only filesystems or in-process brokers as a sandbox.
* A future adapter must add a new ADR, runtime probe, security lane and negative
  tests before flipping `Unavailable` to `Available`.
* Platform-specific reasons remain actionable but do not leak secrets or host
  layout details.

## Tests

* `strict_mode_refuses_without_isolation` (CLI): refusal, exit 1, no stdout,
  JSON error envelope.
* `platform_reports_strict_unavailable` (CLI): `inspect --platform --json`
  reports `state: unavailable` with a non-empty reason.
* Existing worker tests remain explicitly labelled crash isolation, not OS
  isolation.
