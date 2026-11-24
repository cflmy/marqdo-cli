# Marqdo developer entry point

Everything a third party needs starts here. The rule behind this page: depend
only on the **released CLI and public protocols**, never on Marqdo's private
Rust structures, `marqdo-ir` internals, `inspect --ir`, relative paths into the
core checkout, or host memory layout. Adding a new domain function must not
require a core commit.

Product version for these docs: **`3.0.0-rc.3`** (Linux x86_64 candidate).
Current protocol contracts: Analysis, Observation, CLI Tool, Plugin ABI v1,
Foreign wire v1, Provider manifest v2, lock v4 and markdown profile
`marqdo.md.v1`. Product, protocol and package versions are independent; see
[compatibility.md](compatibility.md).

## Pick your entry point

| Goal | Use | Deliverable |
|---|---|---|
| AI/math/data/quantum/SKILL libraries, pure algorithms | `.mq.md` package | `index.mq.md`, `marqdo.yaml`, README, tests |
| Knowledge/Proof/View/IDE/lint/analysis tools | CLI Tool protocol | standalone executable + `tool.json` + Analysis/Observation consumer |
| High-performance native callable functions | Callable Plugin ABI v1 | `plugin.yaml`, host-targeted library, SDK/header |
| New foreign language backend (R/Python/Julia/…) | Foreign Kernel | `kernel.json`, executable provider, pinned environment |
| Hardware/device/external-stream resources | Resource Provider v2 | `provider.json`, C ABI library, worker helper |
| Change `@exec` semantics, add keywords/types | **not** a third-party extension | core language RFC + migration + full conformance |

## Recommended reading order

1. [quickstart.md](quickstart.md) — install the archive and run the first
   project.
2. [libraries.md](libraries.md) — publish a pure `.mq.md` library and consume
   it through a pinned Git dependency.
3. [tools.md](tools.md) — build an external Tool that consumes the current
   Analysis and Observation contracts.
4. [extensions.md](extensions.md) — plugins, foreign kernels, resource
   providers, extension selection and lock v4.
5. [analysis-observation.md](analysis-observation.md) — the public static/dynamic data
   contracts and digest binding.
6. [compatibility.md](compatibility.md) — version axes, platform matrix and the
   prerelease policy.
7. [release-limits.md](release-limits.md) — the honest beta limitations (this
   file ships in the archive as `KNOWN_LIMITATIONS.md`).

The full worked handbook is
[`../MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md`](../MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md)
(beta-era integration handbook, kept as a historical record; the current
contracts are the pages above and `spec/`).
Normative contracts live in [`../../spec/`](../../spec/); this directory is a
task-oriented map, not a second specification.

## Baseline repository

The current implementation and release entry point is
[`git.cflmy.cn/cflmy/marqdo-cli`](https://git.cflmy.cn/cflmy/marqdo-cli). The
earlier `cflmy/marqdo` repository is a different, incompatible language/product
line; it is not a source of current protocol or syntax truth. See the parent
repository README for the repository split.

## Testing against a frozen binary

Any external repository should be testable without rebuilding the CLI:

```bash
export MARQDO_BIN=/absolute/path/to/marqdo
# Put marqdo-provider-worker next to it for provider tests, exactly as the
# release archive does.
$MARQDO_BIN --version
```

Compute the binary SHA-256 before and after your suite; it must not move. The
reference implementation of this gate is
[`marqdo-extension-conformance`](https://git.cflmy.cn/cflmy/marqdo-extension-conformance)
and `tools/ci/verify_extension_contracts.py` in the core repository.
