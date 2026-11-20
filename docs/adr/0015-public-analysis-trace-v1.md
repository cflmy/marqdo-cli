# ADR-0015: Public Analysis v1 and Trace v1

Status: **Accepted**
Date: 2026-11-20

## Context

Knowledge/Proof/View must consume semantic data without linking private Rust
crates or reverse-engineering `inspect --ir`. M0.5 has no stable projection and
the internal IR may change freely. At the same time tracing execution must not
become a second interpreter or leak values/secrets.

## Decision

### Analysis v1

`marqdo check <file> --emit <path>` writes `spec/analysis-v1.md`:

* read-only, static, no execution/plugin dlopen/foreign spawn;
* package-aware module identity, source digests, units, function signatures,
  normalized body nodes, imports, resolved/unresolved/external references,
  call edges and diagnostics;
* deterministic byte-for-byte output, atomic write, no artifact on error;
* `schema_version` is a major version; additive optional fields are allowed,
  removals/changes require a bump;
* normalized nodes (`literal`, `variable`, `binary`, `call`, `assign`,
  `branch`, `loop`, `return`, ...) are the contract, not Rust enum serde.

### Trace v1

`marqdo run/call --trace-file <path>` writes `spec/trace-v1.md`:

* optional, versioned, atomic; only execution produces it;
* semantic events from the interpreter and effect events from the host share a
  global sequence; function ids are canonical;
* values, secrets and handles are never recorded; effect detail is only a
  result summary;
* a failed run records its terminal status; it is never marked successful;
* `source.analysis_digest` lets consumers detect a stale trace;
* existing `--trace` human/JSON effect output is unchanged.

## Alternatives

1. Expose `marqdo-ir` serde directly. Rejected: locks external repos to Rust
   implementation details and private optimization.
2. Make View call `run` in read-only mode to derive structure. Rejected:
   read-only rendering must not execute; use Analysis.
3. Put arguments/values in traces. Rejected by default privacy policy.
4. Add `analyze`/`trace` top-level commands. Rejected: `check --emit` and
   `--trace-file` are flags on existing paths.

## Compatibility

New optional CLI flags and new spec files. No existing language semantics
change. Internal IR changes must keep the protocol tests green; breaking the
public schema requires Analysis/Trace v2 and version negotiation.

## Security

Both exporters are subject to the static/execution boundary: Analysis never
executes; Trace only observes an already-authorized run. Knowledge/Proof/View
gain no capability from reading either artifact. A trace is evidence of an
observed run, not a proof of purity or correctness.

## Testing

`crates/cli/tests/analysis_trace.rs` checks determinism, identity, semantic
nodes, stale-artifact removal, trace sequencing/status and value redaction.
`tests/consumers/analysis_consumer.py` is an independent Python consumer.

## Migration & rollout

R4 implements both protocols for the current language subset. Proof-specific
extensions or richer View events are additive optional fields or a v2 ADR; the
core does not add Knowledge/Proof/View business logic.
