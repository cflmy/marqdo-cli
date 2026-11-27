# Compatibility, versioning and support

This page summarizes the public support policy. The authoritative matrix is
[`../../COMPATIBILITY.md`](../../COMPATIBILITY.md); the beta policy is
[`../MARQDO_3_0_PRERELEASE_PLAN.md`](../MARQDO_3_0_PRERELEASE_PLAN.md).

## Version axes (do not conflate)

| Axis | `3.0.0-rc.6` value | Change rule |
|---|---|---|
| Product / CLI | `3.0.0-rc.6` (candidate) | SemVer prerelease. See `CHANGELOG.md` for changes from the beta. |
| Analysis | v3 | One writer and one current major for every project. New major for incompatible changes; consumers reject unknown majors. |
| Observation | v1 | One `run_id`/`seq` event sequence per execution; JSONL, JSON snapshot and file snapshot come from the same run. New major for incompatible required-field/ordering/redaction changes. |
| CLI Tool | revision 1 | Exact `requires.api_revision` plus optional exact `analysis`/`observation` integers; no version arrays or negotiation. |
| Plugin ABI | v1 (`1.0`) | Frozen C layout and version negotiation. |
| Foreign wire | v1 | One-shot request envelope. |
| Provider manifest | v2 | `schema_version: 2`, `abi: c-abi-1`, `transport: worker`. |
| Lock | v4 | v3 requires an explicit `marqdo lock` migration; no silent reinterpretation. |
| Markdown profile | `marqdo.md.v1` | Frozen parser/options; the RFC-LANG-0002 mapping rewrite did not change the profile id. |
| Bundled stdlib | `0.1.0` | Independent package version; not the product version. |
| Plugin/tool/example packages | their own | Independent SemVer. |

`project.marqdo` is an **enforced product range**; a tool declares its exact
current protocol requirements in `requires`. Prerelease cores require a prerelease range such as
`>=3.0.0-beta.1,<4.0.0`; `>=3.0.0,<4.0.0` does not match the RC.

## Candidate platform scope

| Platform | Release status | Notes |
|---|---|---|
| Linux x86_64 (glibc 2.39 baseline) | **RC target** | Only release archive: CLI + provider worker, same target triple. |
| Linux aarch64 | Not built | No artifact; source may build. |
| macOS arm64/x86_64 | Experimental, not claimed | Core may run; no macOS provider worker, no plugin ABI claim, no archive. |
| Windows x86_64 | Experimental, not claimed | Compile gate only; C ABI/plugin/provider/worker not claimed/fail-closed. |

Strict is Linux-only and probe-gated. Plugin/provider worker transports are not
OS sandboxes. `pure`/`project` are execution modes, not security boundaries.
Observation v1 is a live observation protocol: it is not a capability grant,
values are off by default, and `--mode strict` refuses `--output jsonl` and
`--observation-file` explicitly (no downgrade).

## Language compatibility

`3.0.0-rc.6` implements RFC-LANG-0002 only. Pre-RFC `.mq.md` source
(Strong-execute / Emph-return / ordered-list branch / bullet-list loop /
block-quote call / first code-span signature) is **not executable**; there is no
`--legacy`, syntax switch or downgrade path. Migration is a rewrite task
documented in [`../migration/README.md`](../migration/README.md).

The earlier `cflmy/marqdo` product line is a different architecture; its source
syntax, package format and plugin ABI v2 are not compatible and no hidden
compatibility layer is provided.

## RC policy

- See [release gates](../releasing.md) for the Linux RC acceptance procedure.
- [Known limitations](release-limits.md) ship with the archive.
- Every archive records its checksum, source SHA and toolchain.
