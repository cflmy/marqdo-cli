# ADR-0068 — One current contract per public boundary

Status: Accepted for implementation on the pre-stable 3.0 line (2026-11-24).

## Context

Issue [#8](https://git.cflmy.cn/cflmy/marqdo-cli/issues/8) identified two
independent Analysis version selectors. A tool could install successfully
even when it could not read the output for a given project. The runtime
also projected the same execution into independently defined Trace and
Execution Events protocols. The compilation pipeline described in
DEVELOPMENT_BLUEPRINT §7 remains the source of language facts.

## Decision

Before the first stable release, replace a broken public contract in place.
For each boundary, keep one current format identifier, one producer and one
validation path. Reject mismatches explicitly. Update the CLI, schemas,
consumers, fixtures and CI in the same integration. Use Git history and
release tags to retrieve retired specifications. A future stable release
requires its own compatibility policy.

- Analysis is always `marqdo.analysis` revision 3, including projects without
  loop transfers. The Markdown AST and IR remain internal authorities.
- Runtime observations use `marqdo.observation` revision 1. One run emits
  one sequence. JSONL stdout and JSON snapshots deliver those same events.
  `run` and `call` use `--output text|json|jsonl` and
  `--observation-file PATH`. Capture selections and budgets apply to all
  observation destinations.
- Tool manifests use `marqdo.cli-tool` revision 2, with a required
  `requires.api_revision: 1` and optional exact Analysis and Observation
  requirements. Reinstall tools registered under earlier formats. The
  existing Tool subcommands and external dispatch suffice for this workflow.
- Keep the three current extension descriptors. Callable exports, resource
  rights and foreign interpreter selection have independent validation and
  lifecycles. Share value encoding and equivalent process transport
  primitives. Keep Lock v4 because its graph and artifact identities still
  describe the selected extensions without a format change.

## Security and verification

Static analysis stays non-executing. Observation binds to the exact Analysis
artifact and defaults to structural capture. Selected values, Text consent,
handles, buffering, output bytes and failure/truncation must follow the
current observation contract. Preserve native trust, process timeouts,
worker quarantine, capability grants, artifact integrity and fail-closed
strict behavior.

Exercise real CLI output with an independent consumer, including old-input
rejection, two simultaneous destinations from a single invocation, source
digest mismatches, allowlisted capture, and consumer installation. Validate
independent View and Lint against the final CLI, then validate the shipped
archive and its SHA-bound release evidence before tagging an RC.
