# ADR-0053: Platform-neutral strict child inspection and the strict `--json` failure contract

Status: **Accepted**
Date: 2026-11-21
Relates to: ADR-0021, ADR-0019, RFC-CI-0001 (B01/B02), `spec/security.md` §1.2
Supersedes: nothing

## Context

Two release-blocking defects were observed at `32d8fe1`:

1. `crates/cli/src/commands/run.rs` called `marqdo_sandbox::marker_present` and
   `verify_active` directly. Those functions are exported only under
   `#[cfg(target_os = "linux")]`, so macOS and Windows builds failed with
   `E0425`. `cfg!(target_os = "linux")` was not a fix: both branches are still
   type-checked.
2. When the strict probe succeeded but Bubblewrap could not construct the
   namespaces at launch time, the supervisor wrote its diagnostic to stderr
   only. In `--json` mode the consumer received an empty stdout and failed to
   parse it (`EOF while parsing a value at line 1 column 0`), even though
   strict correctly refused to execute project code.

The same design also let probe-time availability and launch-time capability
drift apart without a deterministic test seam for the failure classification.

## Decision

### 1. One platform-neutral child API

`marqdo-sandbox` exposes `StrictChildState { NotChild, VerifiedChild }` and
`inspect_strict_child()`. Linux performs the existing marker **and** identity
verification (PID 1, NoNewPrivs, empty CapEff, unshared uid map, no
non-loopback interface). Every other platform returns `NotChild` only when the
marker is absent, and returns an error when `MARQDO_STRICT_ACTIVE` is set: a
forged marker never authorizes strict execution. `marker_present` and
`verify_active` are no longer re-exported from the crate root.

### 2. Capture, classify, then route

The CLI uses `launch_strict_capture` and classifies the captured outcome before
writing anything. The `StrictOutcome` type is platform-neutral; non-Linux
builds compile the same routing code and receive `StrictUnavailable` instead of
a Linux-only symbol.

Control-plane failures (probe, spawn, namespace construction, timeout, capture)
are never forwarded as child output. In `--json` mode they emit exactly one
document:

```json
{"command": "run", "success": false, "error": "...", "code": "strict.spawn_failed"}
```

A verified child's successful `--json` output is forwarded unchanged. Anything
that is not exactly one JSON document is rejected as `strict.child_invalid`;
raw Bubblewrap text is never emitted as the program's stdout. A child that
produced valid program JSON keeps its own exit code.

Stable codes:

| Code | Meaning |
|---|---|
| `strict.probe_unavailable` | strict isolation is not available on this host |
| `strict.spawn_failed` | the supervisor process could not be spawned |
| `strict.namespace_failure` | the supervisor could not construct namespaces |
| `strict.timeout` | the supervisor exceeded its wall-clock budget |
| `strict.resource_limit` | a signal/rlimit terminated the supervised child |
| `strict.child_invalid` | the child produced no single JSON document |
| `strict.identity_invalid` | a marker was present without a verified identity |
| `strict.unsupported_grant` | strict L1 rejected an external grant flag |

The names are frozen by this ADR; adding a code requires a new ADR or an
explicit amendment.

### 3. Fail closed remains absolute

Probe success is not a promise. A launch failure after a successful probe is a
strict refusal, never a fallback to `pure`/`project`, and never executes project
code without the supervisor. The injectable `run_with` seam in
`crates/marqdo-sandbox/src/linux.rs` keeps that path deterministically testable.

## Consequences

* macOS/Windows compile the same CLI run path; they still report strict as
  unavailable and never fake an isolation adapter.
* `--json` consumers always parse one document, on every platform and every
  refusal path covered by the contract.
* Failure classification is testable without relying on a host's namespace
  capacity; the disposable-VM lane remains the authority for real isolation.
