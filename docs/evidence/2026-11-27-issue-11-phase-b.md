# Issue #11 Phase B — the accepted Bundle is the ecosystem artifact

Date: 2026-11-27.
Decision: [ADR-0070](../adr/0070-explicit-stdlib-bundle-build-input.md).
Parent: ecosystem issue [#9](https://git.cflmy.cn/cflmy/marqdo/issues/9) and
CLI issue [#11](https://git.cflmy.cn/cflmy/marqdo-cli/issues/11).

This is a **candidate record** for the Phase B change set. The repository has no
`stdlib/` source tree; the accepted Bundle identity is pinned to the ecosystem
root commit. CI evidence is recorded on the PR; the numbers below are the local
candidate host results.

## 1. Bundle identity

```json
{
  "schema": "marqdo.stdlib-bundle-pin",
  "schema_version": 1,
  "name": "marqdo-stdlib",
  "version": "0.2.0",
  "source": {
    "kind": "ecosystem",
    "repository": "cflmy/marqdo",
    "commit": "48189243f747d5f73d5a38a565010e64e2db74e7",
    "producer": "scripts/build-stdlib.py"
  },
  "tree_digest": "sha256:db905508dde89763b9d43b47fc5d52c6a1d5445a91f3c86a550ced43e8aac9d7",
  "aliases_digest": "sha256:5f513c0fdbb3c1e9829be3c9fa23a2d1e89cb8510073fb5eb1e8e2f5265a83bb"
}
```

`tools/ci/fetch_ecosystem_bundle.py` fetches that root commit, initializes the
six stdlib component gitlinks it records, runs the root-owned
`scripts/build-stdlib.py`, and verifies the produced Bundle against the pin.
The CLI never builds the Bundle from a branch or an unknown checkout.

## 2. Source ownership after Phase B

| Source | Owner |
| --- | --- |
| 17 default-Bundle modules (`clock`, `datetime`, `env`, `process`, `random`, `format`, `io*`, `path`, `fs/file`, `data/json`, `data/csv`, `collections/table`, `data/table`, `text*`) | six `marqdo-std-*` repositories |
| HTTP / regex / gzip domain modules | `marqdo-ext-http`, `marqdo-ext-regex`, `marqdo-ext-archive` explicit Packages |
| Module self-tests and domain behaviour tests | the owning component repository |
| Bundle assembly, digest/alias manifest, composition acceptance | `cflmy/marqdo` (root) |
| CLI loader/runtime, `std:` resolution, official alias routing, generic resources | `marqdo-cli` |

Removed from the CLI: `stdlib/` sources and self-tests, the debug source
fallback, `stdlib_http.rs`/`stdlib_table.rs`/`http_over_tcp.rs` domain tests and
the `stdlib/demo.mq.md` formatting test. The print/format equality and Table
split assertions moved to the root composition lane; the generic TCP round-trip
stays a CLI test (`tcp_round_trip.rs`); the HTTP loopback client test follows
the HTTP package owner (tracked as a follow-up on `marqdo-ext-http`).

## 3. Local candidate verification

Host: Linux x86_64, `rustc 1.100.0`, Python 3.14; Bundle materialized by
`tools/ci/fetch_ecosystem_bundle.py --out /root/work/marqdo/.build/cli-ecosystem-bundle`.

| Gate | Command | Result |
| --- | --- | --- |
| Bundle identity | `python3 tools/stdlib/bundle.py --stdlib-dir <bundle>` | PASS (matches pin) |
| Formatting | `cargo fmt --all -- --check` | PASS |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| Workspace tests | `MARQDO_TEST_STDLIB_BUNDLE=<bundle> cargo test --workspace --locked` | 72 test binaries, 0 failed |
| Python tool tests | `MARQDO_TEST_STDLIB_BUNDLE=<bundle> python3 -m unittest discover -s tools/ci -p "test_*.py"` | 75 tests, OK |
| Product lane | `cargo test -p marqdo-cli --test stdlib_product` | 4 tests PASS (surface, aliases, `std:` execution, missing-Bundle fail-closed) |
| Independent build | `python3 tools/ci/independent_build.py` | see CI/PR record |

## 4. Unverified / follow-ups

- CI execution of this candidate is the gating evidence; it is recorded on the
  PR rather than in this file.
- Release archive cold-install with the new Bundle and the root candidate
  promotion are part of MIG-RELEASE (ecosystem issue #9), not this candidate.
- HTTP-over-TCP domain test migration to `marqdo-ext-http` and multi-platform
  release evidence remain open; they are not claimed here.
