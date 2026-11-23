# Linux RC1 assessment for RFC-CTRL-0005

Candidate `af1e07f3d772a7f641c37ef9c2d435783f93141d` was tested on
`release/3.0.0-rc.1-control-flow`. This is a separate verification branch
from the existing `release/3.0.0-rc.1` freeze at `f7a5a7b`.

[Woodpecker #56](https://ci.cflmy.cn/repos/1/pipeline/56) passed all steps,
including format, Clippy, workspace tests, Windows cross-compilation, the
Rust 1.89 locked check, release build, protocol smoke, and independent extension
conformance. Windows and macOS are outside this Linux candidate's scope.

## Archive

The locally rebuilt archive passes its checksum, smoke tests and installed-only
cold check in Ubuntu 24.04 with glibc 2.39. Its seven frozen extension contract
groups and six reference applications passed. Its executable digests match
the two digests stamped by Woodpecker #56.

| File | SHA-256 |
|---|---|
| Linux archive | `7d04070dfea78c71a4223b85156ec18b18476410d3a28db4b4fe24ab2dd7f5cf` |
| `marqdo` | `adaea5d34f914878b3ee67f8ea852741a6021d07e42e4e0d7e10510ee4559c8a` |
| `marqdo-provider-worker` | `71f145e75d0f69b7a292d7a48da0bf1f850d6f57efb417f59877c8acdeb195d3` |

The archive declares Analysis v2 and v3. Its smoke test exercises both majors
against independent readers and checks the Trace v2 digest. The old Lint
consumer limitation is recorded in the
[control-flow development verification](2026-11-23-rfc-ctrl-0005.md).

## Strict L1

The strict security lane passed for this SHA in a disposable Ubuntu 26.04 VM
as uid 1000, with Bubblewrap 0.11.1 and kernel 7.0.0-43-generic. The base image
digest was verified against Canonical's dated release checksum. The same guest
also verified the archive checksum and ran the installed example in strict
mode, returning 5 with the CLI digest shown above.

## Performance

Each paired run compared the beta tag with the extracted candidate archive.
All nine workload results were correctness-verified on Woodpecker agent 1,
pinned to logical CPU 0, kernel 7.0.0-31-generic, on an Intel i7-12700H with the
`powersave` governor. Each run used five warmups and twenty samples per
workload. All three candidate executable digests match the archive above.
The complete beta and candidate reports are retained in the linked logs.

| Paired run | HTTP parser p50 change | HTTP parser beta and candidate p95 |
|---|---:|---:|
| [#57](https://ci.cflmy.cn/repos/1/pipeline/57) | +21.58% | 5.5765 / 5.6545 ms |
| [#58](https://ci.cflmy.cn/repos/1/pipeline/58) | +0.05% | 5.7052 / 5.6888 ms |
| [#59](https://ci.cflmy.cn/repos/1/pipeline/59) | +0.15% | 5.7086 / 5.6371 ms |

The first pair crossed the 15% p50 threshold. Its beta p50 was 4.5126 ms,
compared with 5.5761 and 5.5602 ms in the next two pairs. The candidate p50
stayed within 5.4865–5.5789 ms across the three. The p95 and CPU samples
overlapped. The threshold breach was not reproduced and is recorded with the
shared-node variability. The largest RSS increase was 1.16%. No other
comparison crossed the investigation thresholds.

The three reports are shared-node SMOKE. The aggregate RC gate records
`DO_NOT_RELEASE` until the maintainer explicitly accepts the named shared-node
WARN or the dedicated-node performance gate passes. The candidate SHA remains
frozen while that decision is pending.
