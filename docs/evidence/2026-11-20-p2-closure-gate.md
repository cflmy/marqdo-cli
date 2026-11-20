# RFC-CM P2 closure gate evidence (C01–C06, C11, C12)

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `d6cd785` plus this step's changes.
Related: [memory P2](2026-11-20-memory-p2-closure.md),
[structure API](2026-11-20-structure-api-p2.md),
[result/free-list](2026-11-20-result-and-free-list-p2.md),
[ADRs 0022–0024](../adr/).
Scope: pure no-Foreign/no-Plugin execution lane; external stdlib repository
publication remains separate.

## Lane

`crates/cli/tests/p2_closure.rs` runs 21 checked-in semantic fixtures through
the real CLI with `run --mode pure --json`:

* `--mode pure` constructs `Capabilities::pure()`; no foreign, plugin, read,
  write, network or provider grant exists in that host;
* each fixture source is scanned for `foreign.`, `plugin.` and `import:`; all
  three must be absent (anti-evasion rule for C01–C06/C11/C12);
* the CLI value and stdout are compared byte-for-byte with the fixture's
  `.expected.json` golden;
* `stdlib/tests/table.mq.md` also runs in pure mode with `std:` imports and
  checks the nine table results including `转GFM`.

The independently-run conformance harness (`conformance_semantics.rs`) also
builds `StdHost` with `SandboxMode::Pure` and `Capabilities::pure()` for every
fixture.

## Criterion mapping

| Criterion | Fixtures | What is proven |
|---|---|---|
| C01 allocators | 48, 50, 55 | bump allocator, failed-allocation atomicity, first-fit split/coalesce/double-free |
| C02 structures | 49, 56, 58, 61 | linked list, binary tree, open-addressing hash table, graph adjacency |
| C03 codecs | 48, 64 | u32, big-endian 16/32/64, LEB128 |
| C04 Unicode/text | 59, 60 | UTF-8 encode and validating decoder errors |
| C05 JSON/CSV | 57, 62, 68 | CSV split/quoting, recursive JSON parse/format/errors |
| C06 GFM Table | 53, 54, 65, 67, 69 + stdlib table | projection/filter/sort/join/group/GFM render, no Rust table algorithm |
| C11 VM | 63 | Memory-backed stack bytecode VM |
| C12 scientific prep | 70, 73 | Int matrix multiply and sparse-triple matrix multiply with dense conversion |

No fixture compiles Rust, imports a package, invokes a foreign kernel or calls a
plugin. Dynamic arrays are built at runtime; no fixed-length source rewrite is
required.

## Gate

Local container: `cargo test -p marqdo-cli --test p2_closure` PASS; full
`cargo test --workspace --locked` PASS. The existing frozen-binary test covers
the fixed CLI hash and stdlib tamper rejection (ADR-0028).

## Remaining (not a C01–C12 gap)

* Publication of `marqdo-stdlib` as a separate external repository with a
  signed version mapping; the bundled tree, digest and offline layout are
  already tested in-repo.
* No GitHub CI URL / Windows/macOS lanes from this host.
