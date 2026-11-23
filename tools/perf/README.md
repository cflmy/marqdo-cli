# Performance sampler (marqdo.perf.v2)

`runner.py` measures a fixed release binary and refuses to report timings for a
workload whose wire output did not match the committed expected digest.

## Rules

* Build one release binary with `cargo build --release --locked -p marqdo-cli`
  and keep it fixed for the whole run. The report records its SHA256, the
  `Cargo.lock` digest, `rustc -vV`, source SHA and profile.
* Correctness comes first: every warmup run is parsed and hashed. A mismatch
  is `CORRECTNESS_FAIL`; its faster wall time is never reported.
* Approved-node release evidence needs at least 5 warmups and 20 samples per
  workload on a dedicated, stable node. A maintainer-approved shared-node
  RC comparison needs the same sample counts in at least three paired runs.
  Shared-node reports stay `SMOKE` and are eligible only for the RC
  `WARN` exception described in [releasing.md](../../docs/releasing.md).
* `--baseline` checks platform ID, build profile, CPU model, kernel,
  governor, CPU affinity and toolchain. For an approved-node comparison
  it also requires an approved, correctness-verified MEASURED baseline.
  Record competing load and investigate differences before sign-off.
* Avoid TCG/QEMU for final performance numbers; use native or KVM with fixed
  CPU affinity (`--cpu-list`) and an administrator-approved governor.
* The suggested investigation thresholds are p50 +15% and peak RSS +20%; they
  are not automatic rollback rules. Investigate before freezing a hard gate.

## Commands

```bash
# Record/review expected wire digests after an intentional behaviour change.
python3 tools/perf/runner.py --record-expected --warmups 1 --samples 1 \
  --allow-smoke --only hello-check

# Non-release smoke run on the development host.
python3 tools/perf/runner.py --warmups 1 --samples 3 --allow-smoke \
  --platform-id local-unapproved --out /tmp/marqdo-perf-smoke.json

# Release measurement on the dedicated node.
python3 tools/perf/runner.py --warmups 5 --samples 20 --approved-node \
  --platform-id linux-x86_64-fixed-node-01 --cpu-list 2,3 \
  --out perf/2026-11-21.json
```

Expected digests live in `expected/<workload>.json` and are versioned with the
source. Running `--record-expected` rewrites them: inspect the diff and commit
it only when the output change is the intended one.

`bench.py` remains only as a compatibility shim.

## RC comparison

On the designated node, run the beta benchmark from a checkout
pinned to `v3.0.0-beta.1` and the candidate benchmark from the frozen
candidate checkout. Use the same toolchain, CPU list and platform ID in both
commands. Pass the extracted, verified candidate CLI via `--no-build --binary`
and add `--baseline /path/to/beta.json` to the candidate command. Keep both
reports and confirm all nine comparisons are present and correctness-verified.
The shared Woodpecker manual workflow runs on master and release branches.
It packages the candidate, checks its checksum and source SHA, then measures
the extracted binary with the shipped stdlib. Shared-node runs produce SMOKE
reports for review under the release policy.
