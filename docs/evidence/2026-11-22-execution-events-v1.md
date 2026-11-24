# Execution Events v1 — local implementation evidence

- **Date:** 2026-11-22
- **Scope:** RFC-OBS-0001 P0 (OBS-00..03) and P1 (OBS-04..05); OBS-06/P2
  memory metadata deliberately deferred.
- **Source SHA:** `1d3dca7c1832eb9468e96384d9805a3812a303e4` (post-beta tree;
  the tagged beta is `2c7c4f31e7a388d3fcfa717a77067e696ed13854` and does **not**
  contain this feature).
- **Normative contract at that time:** `spec/execution-events-v1.md` (retired
  by [ADR-0068](../adr/0068-single-current-public-contracts.md); the current
  contract is [`spec/observation.md`](../../spec/observation.md)), decision
  [ADR-0059](../adr/0059-runtime-observability-and-execution-events.md).

This file records commands actually executed on Linux x86_64 with Rust
1.100.0. It is local evidence, not a same-SHA CI or published-release claim.

## Static and test gate

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS: 630 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test execution_events      PASS: 14 passed
cargo test -p marqdo-host --test observation          PASS: 8 passed
cargo test -p marqdo-runtime --test observation       PASS: 4 passed
cargo test -p marqdo-cli --test docs_consistency      PASS
cargo test -p marqdo-cli --test architecture_boundaries PASS
```

The `execution_events` suite covers: pure JSONL envelope, contiguous `seq`,
terminal ordering, byte-exact base64 reconstruction (including invalid UTF-8
and multi-byte text), live `READY` delivery while the child is still running,
prepare/runtime terminals, exact value selection and text redaction, Trace v2
coexistence/shared run id, structural-budget truncation, output-budget
fail-closed, the checked-in JSON Schema fixtures, the structure fixture, and
broken-pipe failure (`observe.transport_failed`, non-zero exit), native-plugin
fail-closed, and observed values never entering Trace v2.

## Frozen-binary black-box consumer

A local release archive was built from the source SHA and the independent
Python consumer was run against the extracted binary's live stream:

```text
archive: dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
archive sha256: d55dd4b3cc3be6d2fec394e870af07fac0dd30bf5eb29f91a73a565ea6381088
extracted marqdo sha256 before: ce0da79024b5677edb04328e56e9d873732b870fd0c3cbd74340c5de6efb073d
extracted marqdo sha256 after:  ce0da79024b5677edb04328e56e9d873732b870fd0c3cbd74340c5de6efb073d (unchanged)
run: marqdo run tests/consumers/fixtures/execution-events/basic.mq.md --event-stream jsonl
run exit: 0
consumer: execution events consumer: ok: 297 events, terminal=run.completed, stdout+stderr bytes=11
```

The stream began with `stream.opened` (`seq 0`), contained exactly one
`run.started`, emitted the two committed stdout chunks before the terminal
event, and ended with `run.completed` (`{"type":"int","decimal":"10"}`).
Concatenated chunk bytes were `START\nDONE\n`, equal to a normal run.

`tools/release/smoke.py --archive ... --skip-strict` on the same archive:
`smoke: PASS` (includes the new live-stream consumer step and `run --help`
protocol option check). The disposable-VM strict lane was not run because this
environment is not a disposable VM; in any case `--mode strict` explicitly
refuses `--event-stream` (structured `run.rejected`, no downgrade).

## Non-claims

- Not included in the `v3.0.0-beta.1` archive and no new release tag was
  created.
- No macOS/Windows native verification for this option.
- No memory metadata/value of containers/handles, no debugger, stepping,
  breakpoints, time travel, raw memory read or bidirectional debug protocol.
- No performance gate; observation is off by default and the disabled path does
  not build observation events, but this file makes no percentage claim.
