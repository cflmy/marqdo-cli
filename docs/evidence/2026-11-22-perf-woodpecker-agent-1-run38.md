# G4 performance evidence — `3.0.0-beta.1`

Date: 2026-11-22. Source SHA: `2c7c4f31e7a388d3fcfa717a77067e696ed13854`. Product: `3.0.0-beta.1`.
Pipeline: Woodpecker manual pipeline #38
(https://ci.cflmy.cn/repos/1/pipeline/38), successful. Report:
[`2026-11-22-perf-woodpecker-agent-1-run38.json`](2026-11-22-perf-woodpecker-agent-1-run38.json)
(schema `marqdo.perf.v2`).

## Environment (fixed node)

```text
platform_id          woodpecker-agent-1
runner_kind          Woodpecker Docker agent (agent_id 1)
kernel               7.0.0-31-generic
arch                 x86_64
CPU                  12th Gen Intel(R) Core(TM) i7-12700H
cpuset / pinned CPU  see report / 0
governor             powersave
loadavg start        5.90 2.76 1.36 1/2879 2894
source_sha           2c7c4f31e7a388d3fcfa717a77067e696ed13854
CI-built binary      sha256:c6ea645ff13e594aa290a1816534351ee25f351962740a3fe5acfddd380ef160
Cargo.lock           sha256:eb9f58c3091c8e95907d7e9ef6d18915c83791826ef00307e818b8ee82f1bb1b
rustc                rustc 1.100.0 (0ef1bc386 2026-11-09)
recipe               {'warmups': 5, 'samples': 20, 'order_seed': '62bd903ba89d915a', 'timeout_seconds': 120.0}
status               MEASURED
correctness          VERIFIED
```

## Results (p50 / p95 ms, MAD, RSS median KiB)

| Workload | p50 | p95 | MAD | RSS | Correctness |
|---|---:|---:|---:|---:|---|
| `hello-check` | 4.8196 | 4.9531 | 0.1178 | 17232 | VERIFIED |
| `http-parser` | 4.7873 | 4.9299 | 0.1333 | 17232 | VERIFIED |
| `table-sort` | 2.3621 | 2.4414 | 0.0289 | 17232 | VERIFIED |
| `factorial-run` | 2.4061 | 2.4723 | 0.0379 | 17232 | VERIFIED |
| `allocator-free-list` | 4.5010 | 4.5861 | 0.0213 | 17232 | VERIFIED |
| `json-parser` | 6.6775 | 6.7354 | 0.0408 | 17232 | VERIFIED |
| `stdlib-print-demo` | 12.4718 | 13.6205 | 0.3838 | 17232 | VERIFIED |
| `matrix-multiply` | 2.3975 | 2.7386 | 0.0313 | 17232 | VERIFIED |
| `bytecode-vm` | 2.4187 | 3.4758 | 0.0627 | 17232 | VERIFIED |

All 9 workloads reported `MEASURED` with `VERIFIED` correctness (immutable
expected wire digests checked before timing). `baseline_comparison` is empty:
this is the first fixed-node baseline for the beta release, so there is no
regression comparison to claim.

## Gate classification

`tools/ci/release_evidence.py --profile beta` records `performance=WARN` for this
release, not `PASS`: the node is a shared Docker container on a powersave host
(loadavg ~5.9), not a dedicated performance machine, and this is a first
baseline. The maintainer-approved Beta-only WARN policy and its limits are in
[`MARQDO_3_0_PRERELEASE_PLAN.md`](../MARQDO_3_0_PRERELEASE_PLAN.md) §3.1 gate 6.
The numbers are valid only for same-`platform_id` regression tracking; they must
not be compared with another machine, container or QEMU/TCG run.
