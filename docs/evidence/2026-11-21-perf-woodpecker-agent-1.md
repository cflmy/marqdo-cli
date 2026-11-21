# G4 performance baseline — Woodpecker agent 1

Status: **MEASURED (provisional baseline); designated fixed agent, shared host
caveat.**
Date: 2026-11-21. Code SHA:
`1199c88e3e461f0443f99c5bf528ddd6a24aa2c6` (CI-only commit; Rust code equals
`a9cc1729e1f6344e1c7a4bd58bc9e4acff4efd9e`).

This is the remote Woodpecker G4 evidence for the designated performance
node. The repository has one Woodpecker agent (`agent_id 1`); it is the fixed
node. It is **not** a physically isolated performance host: it is a Docker
container on a shared machine, the CPU governor is `powersave`, and the host
load average was about 5.7–6.1 during both runs. The numbers are valid only for
same-`platform_id` regression tracking; do not compare them with another
machine, another container, QEMU/TCG, or a future dedicated node.

## Fixed node and recipe

```text
platform_id          woodpecker-agent-1
runner_kind          Woodpecker Docker agent (agent_id 1)
container hostname   c9301248e4c7
kernel               7.0.0-31-generic
arch                 x86_64
CPU                  12th Gen Intel(R) Core(TM) i7-12700H
cpuset (allowed)     0-19
pinned CPU           0 (taskset -c 0)
governor             powersave
source_sha           1199c88e3e461f0443f99c5bf528ddd6a24aa2c6
binary               sha256:15ead6261c7500eb3fbcb7c9f60b76abf85b4006883dd23850f74a43af5b18c0
Cargo.lock           sha256:5b9754a3ff7f56ca0158d70772365f02dcbfbc1ebbd97a55ee09d441c2d2af01
rustc                rustc 1.100.0 (0ef1bc386 2026-11-09)
recipe               5 warmups + 20 samples per workload, release profile,
                     SHA-derived interleaved order, 9 workloads
status               MEASURED
correctness          VERIFIED for all 9 workloads
```

Both runs passed the immutable expected wire digests in
`tools/perf/expected/`; a wrong output would have been `CORRECTNESS_FAIL` and
its timing would not have been reported.

## Runs

| Run | Woodpecker pipeline | Start loadavg | Notes |
|---|---|---|---|
| 1 | [#31](https://ci.cflmy.cn/repos/1/pipeline/31) | 6.13 | first baseline |
| 2 | [#32](https://ci.cflmy.cn/repos/1/pipeline/32) | 5.66 | repeatability check |

Reports (full `marqdo.perf.v2` JSON):

* [`2026-11-21-perf-woodpecker-agent-1-run31.json`](2026-11-21-perf-woodpecker-agent-1-run31.json)
* [`2026-11-21-perf-woodpecker-agent-1-run32.json`](2026-11-21-perf-woodpecker-agent-1-run32.json)

## Results (run 31, p50 / p95 ms)

| Workload | p50 | p95 | MAD | RSS median (KiB) |
|---|---:|---:|---:|---:|
| `hello-check` | 4.8822 | 4.9401 | 0.0396 | 17676 |
| `factorial-run` | 2.3949 | 2.4406 | 0.0244 | 17676 |
| `allocator-free-list` | 4.5207 | 4.5971 | 0.0323 | 17676 |
| `json-parser` | 6.6149 | 6.8380 | 0.1795 | 17676 |
| `table-sort` | 2.3660 | 2.4724 | 0.0254 | 17676 |
| `bytecode-vm` | 2.3634 | 2.4443 | 0.0384 | 17676 |
| `matrix-multiply` | 2.5671 | 2.6745 | 0.0744 | 17676 |
| `http-parser` | 4.5041 | 4.5946 | 0.0347 | 17676 |
| `stdlib-print-demo` | 11.9034 | 12.0538 | 0.1354 | 17676 |

Repeatability: run 32 p50 deltas versus run 31 ranged from **-10.89% to
+7.10%**, all below the tool's 15% investigation threshold. Peak RSS was
17348 vs 17676 KiB (-1.9%). This supports using run 31 as the first baseline;
it does not replace the RFC's multi-day baseline requirement.

## Gate classification

`tools/ci/release_evidence.py` records this gate as **WARN**, not PASS, for
`1199c88`: the report is `MEASURED` with verified output, but the node is a
shared, powersave-governed agent and this is the first same-day baseline. The
release manifest is consequently `DO_NOT_RELEASE` only because `strict_vm` is
still `UNVERIFIED`; see
[`ci/1199c88e3e461f0443f99c5bf528ddd6a24aa2c6/manifest.json`](ci/1199c88e3e461f0443f99c5bf528ddd6a24aa2c6/manifest.json).

## How to rerun

```bash
curl -X POST \
  -H "Authorization: Bearer $MARQDO_WOODPECKER_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"branch":"master","workflow":"performance","message":"G4 baseline"}' \
  "https://ci.cflmy.cn/api/repos/1/pipelines"
```

The workflow is manual/master-only and never runs PR code. It builds one
release binary, pins CPU 0 from the container's allowed cpuset, runs
`runner.py --no-build --warmups 5 --samples 20 --approved-node --platform-id
woodpecker-agent-1 --cpu-list 0`, and prints the report between
`G4_REPORT_BEGIN`/`G4_REPORT_END` in the step log.
