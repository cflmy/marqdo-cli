# Architecture gates

The executable architecture checks are Rust integration tests in
`crates/cli/tests/`, so they run inside `cargo test --workspace`:

| Gate | Test | What it pins |
|---|---|---|
| source dependency graph | `crates/cli/tests/architecture_boundaries.rs` | workspace members, `marqdo-runtime -> marqdo-host`, no back-edge, no `marqdo-stdlib` crate |
| bundled stdlib identity | `crates/cli/tests/architecture_boundaries.rs` | `stdlib.json` version + SHA-256 digest over sorted `.mq.md`, `std:`/relative imports stay inside `stdlib/` |
| public resource endpoints | `crates/cli/tests/architecture_boundaries.rs` | `stdio/stdin/stdout/stderr`, `file`, `tcp` remain in `marqdo-host/src/resource.rs` |
| documentation consistency | `crates/cli/tests/docs_consistency.rs` | one current overview, status/index links, evidence SHA pinning, no stale `c9f01aa8` claim |
| primitive-only programs | `crates/cli/tests/p2_closure.rs` | C01–C06/C11/C12 run in `--mode pure` with no Foreign/Plugin/import shortcut |
| strict refusal matrix | `crates/marqdo-sandbox/tests/security_linux.rs` | strict L1 refuses unverified resources; G-C VM lane |

A `.mq.md` demo is not an architecture gate. Put semantic fixtures under
`tests/conformance/`, product tests under `stdlib/tests/`, and OS-isolation
commands under `tests/security/`. Do not move a Rust gate into this directory;
Cargo would stop running it.
