# Evidence: documentation and test-case migration

Date: 2026-11-21 (Asia/Shanghai)
Source/test baseline: `0d40de7645fb15b09bb82a27907ae63ba3c4159f` (docs/test migration;
later docs-only commits keep code and test bytes identical)
Repository: `marqdo-cli`
Decision input: architecture audit and refactor RFC, P0/P1/P2

## Commands

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                               PASS
  463 passed, 0 failed, 2 ignored
cargo test -p marqdo-cli --test docs_consistency       4 passed
cargo test -p marqdo-cli --test architecture_boundaries 5 passed
cargo test -p marqdo-cli --test stdlib_product          5 passed
python3 tools/pack-stdlib.py                           PASS
```

`0d40de7645fb15b09bb82a27907ae63ba3c4159f` is the source/test commit before this evidence edit; later commits
only add/edit documentation and the status text. Code and test bytes are unchanged.

## Doc gate

`crates/cli/tests/docs_consistency.rs` checks:

* `docs/architecture/CURRENT.md` has one current overview, the four state
  dimensions (implemented / tested locally / bundled stdlib released /
  multi-platform verified) and a 40-hex baseline SHA;
* `docs/architecture/README.md` is an index that links `CURRENT.md`,
  `STATUS.md` and `history/README.md`, without a second running P5 status;
* `docs/STATUS.md` links `CURRENT.md` and no longer claims the stale
  `c9f01aa8` remote baseline as current;
* relative Markdown links in those files resolve;
* every `docs/evidence/*.md` file contains a 7–40 hex commit/security SHA.

## Architecture gate

`crates/cli/tests/architecture_boundaries.rs` checks:

* the workspace members are the documented 15 members, with no
  `marqdo-stdlib` crate;
* `marqdo-runtime` depends on `marqdo-host` and `marqdo-host` does not depend
  back on `marqdo-runtime`;
* `stdlib.json` is version `0.1.0` and its SHA-256 digest matches the sorted
  `.mq.md` tree;
* `std:`/relative imports stay inside `stdlib/` and resolve;
* `marqdo-host/src/resource.rs` still exposes `stdio/stderr/file/tcp`.

`tools/pack-stdlib.py` used `Path` order while the CLI used relative-path
order, so the two digest definitions disagreed on `io.mq.md` vs `io/console.mq.md`.
The tool now sorts by the same relative key; it verifies the manifest again.

## Product test gate

`crates/cli/tests/stdlib_product.rs` requires one `stdlib/tests/*.mq.md` for
every public module and runs all of them through the real `std:` import path with
the verified bundle:

| Test | Public module |
|---|---|
| `console.mq.md` | `std:io/console` |
| `demo.mq.md` | `std:demo` |
| `format.mq.md` | `std:format` |
| `http.mq.md` | `std:net/http` |
| `io.mq.md` | `std:io` |
| `stream.mq.md` | `std:io/stream` |
| `table.mq.md` | `std:data/table` |
| `text.mq.md` | `std:text` |
| `utf8.mq.md` | `std:text/utf8` |

The tests assert the value report, `success`, exact stdout, and a resource
endpoint in the trace for console/stream.

## Not covered

A conformance fixture is not a product test. No `std:fs/file`, JSON/CSV,
directory/stat/rename/remove, DNS/TLS/UDP/listener, `std:path/regex/datetime/
env/process`, secure random, compression, Windows/macOS native lane, or GitHub
security URL is added here. Those remain open in `../STATUS.md` §4.
