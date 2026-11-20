# RFC-CM-0001/0002 completion audit

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Final gate: `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test --workspace --locked` (440 passed, 0 failed, 2 ignored).
Strict VM lane: `G-C security lane: PASS` at `70d916d`
([log](2026-11-20-strict-resource-cm-a11.log)).

This audit maps the objective bullets to the implementation, tests, ADRs and
checked-in evidence. It also states exactly what remains external.

## Objective checklist

| Objective item | State | Evidence |
|---|---|---|
| P2 closure C01–C06/C11/C12, JSON floats/table group/no-foreign | **PASS local** | 21-fixture pure lane `crates/cli/tests/p2_closure.rs`, fixture 71/73, JSON floats/table group, foreign/plugin negatives; [P2 gate](2026-11-20-p2-closure-gate.md) |
| Standalone stdlib packaging + frozen-CLI closure | **PASS** | ADR-0028; `crates/cli/tests/stdlib_bundle.rs`; deterministic archive `tools/pack-stdlib.py` + [package evidence](2026-11-20-stdlib-package.md) |
| CM-A07 Compat-B/C print migration | **PASS** | ADR-0030/0031; `print_routing.rs`, `conformance_semantics`, `HostOp::Print` removed |
| CM-A08 Table semantics | **PASS local** | ADR-0032; `stdlib/data/table.mq.md`, `stdlib_table.rs`, fixture 72 cross-storage, `转GFM` |
| CM-A09 provider ABI | **PASS local** | ADR-0033 in-process, ADR-0035 worker helper, ADR-0036 C ABI, ADR-0037 package manifest, ADR-0038 C ABI worker binding; host/provider tests |
| CM-A10 offline stdlib packaging | **PASS local** | bundled tree + digest + Analysis identity + frozen binary; deterministic offline archive; external repository publication remains external |
| CM-A11 strict resource verification | **PASS (VM)** | strict lane at `70d916d`: 7/7 adversarial, Compat-C print, file/TCP/provider refusal, `--allow-net` rejection, forged marker; [evidence](2026-11-20-strict-resource-cm-a11.md) |
| P4 table logical/physical | **PASS local** | row-store vs columnar fixture 72; GFM render; no Rust table algorithm |
| P5 provider/TCP/HTTP | **PASS local** | in-process/worker/C provider, IP-literal TCP, `.mq.md` HTTP framing + loopback HTTP over TCP |
| P6 as far as environment permits | **BASELINE** | `tools/perf/bench.py`, 7-workload [profile](2026-11-20-p6-profile.md); no optimizer/backend claimed |

## Commit map

`92bfbf2` baseline P0–P3, `5d3a5fb` P2 evidence, `1a978c7` JSON floats,
`1377af1` bundled stdlib, `e119727` full-value formatting, `e26971f`
Compat-B, `49910ec` Compat-C, `a8e62a5` table stdlib, `1be4662` in-process
provider, `1d87f98` strict stdlib mount, `00bdcad` TCP, `67a6d35` strict
evidence, `69a28ed` table GFM + HTTP fixture, `d6cd785` status, `0d4dc1d`
P2 closure lane, `e981168` HTTP over TCP, `b020904` cross-storage, `4c8e7fd`
P6 baseline, `b241d1d` stdlib package, `3484ab9` strict refresh, `8bbed34`
sparse matrix, `5ad2068` counts, `70d916d` strict TCP, `0946220` strict
evidence, `12be02e` P4/P5 gates, `01f6fe4` worker provider, `b22a89e` C ABI,
`d909de5` status, `5e5c3a8` C fail-closed, `313ac03` P2 anti-evasion,
`974102d` provider package, `757c5ed` C worker binding.

## External / future items (not available or not claimed)

1. **No GitHub credentials / remote CI URL.** Windows/macOS native lanes and a
   self-hosted G-C CI URL cannot be produced from this host. Strict evidence is
   local VM evidence, not CI-signed.
2. **No separate external `marqdo-stdlib` repository publication.** The
   deterministic offline package and digest contract exist; publishing/signing
   the external repo is outside this host.
3. **Existing plugin-Worker ABI provider binding** is not implemented; provider
   ABI is delivered via ADR-0033/0035/0036/0037/0038.
4. **P6 optimizer/backends** are not implemented; only a reproducible baseline
   profile exists. RFC P6 is a post-stabilization performance phase.
5. Provider registry/download/signatures and strict-mode providers are future.

No capability above is claimed as CI-verified or externally published.
