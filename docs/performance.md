# Performance measurements (G4)

Status: the sampler and correctness corpus are implemented; **no release
performance baseline exists yet**. Do not cite a development-container or
TCG/QEMU number as a release gate.

## Three separate measurements

| Class | Goal | Host |
|---|---|---|
| Language core throughput | interpreter, expressions, memory/containers/stdlib algorithms | fixed G4 Linux node |
| CLI experience | cold `check`/`run`/`inspect`, parsing, stdlib loading | fixed G4 Linux node |
| Strict overhead | probe, re-exec, verification and strict run time vs pure | separate isolated performance node |

The G3 security VM can report correctness, not release performance: TCG time is
dominated by emulation and is not comparable with native or KVM.

## Tool

`tools/perf/runner.py` (schema `marqdo.perf.v2`) builds one release binary,
records its SHA256/Cargo.lock/rustc/source SHA, verifies every workload's exact
wire digest **before** timing, then reports warmups/samples, p50, p95, MAD,
CPU user/sys, peak RSS and the environment manifest. See
[`tools/perf/README.md`](../tools/perf/README.md).

Rules enforced by the tool:

* a wrong workload output is `CORRECTNESS_FAIL`; its timing is never reported;
* fewer than 5 warmups / 20 samples is `SMOKE`, not release evidence;
* `--approved-node` is required for `MEASURED`;
* a baseline from another `platform_id` or profile is refused;
* p50 +15% / RSS +20% are investigation thresholds, not automatic rollbacks.

## Current evidence

* Development-container smoke run (2026-11-21, `--warmups 1 --samples 3
  --allow-smoke`): all nine workloads `VERIFIED`/`SMOKE`, no correctness
  failures. Stored as a smoke artifact, **not** a release baseline.
* Remote G4: **UNVERIFIED** until a fixed self-hosted node runs
  `.github/workflows/performance-linux.yml` with a fixed `--cpu-list`.
