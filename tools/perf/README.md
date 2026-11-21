# Performance sampler (marqdo.perf.v2)

`runner.py` measures a fixed release binary and refuses to report timings for a
workload whose wire output did not match the committed expected digest.

## Rules

* Build one release binary with `cargo build --release --locked -p marqdo-cli`
  and keep it fixed for the whole run. The report records its SHA256, the
  `Cargo.lock` digest, `rustc -vV`, source SHA and profile.
* Correctness comes first: every warmup run is parsed and hashed. A mismatch
  is `CORRECTNESS_FAIL`; its faster wall time is never reported.
* Release evidence needs at least 5 warmups and 20 samples per workload with
  `--approved-node` on a dedicated, stable performance node. Shorter runs are
  `SMOKE` and must not be cited as a release gate.
* Do not compare timings across different `platform_id`s, CPU models, kernels,
  toolchains or load conditions. `--baseline` refuses a different
  `platform_id`/profile.
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
