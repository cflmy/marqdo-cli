# Embedding consumer

This is a real, independent Cargo package, **not** a workspace member. It is
the engine-side acceptance consumer for issue #13 / ADR-0071: it builds and
executes the same `.mq.md` program through the real Markdown → frontend → IR →
runtime path while depending only on the platform-neutral engine crates.

It must never depend on `marqdo-host`, `marqdo-sandbox`, `marqdo-foreign`,
`marqdo-plugin-abi`, the CLI or a native loader. `tools/ci/check_engine_boundary.py`
builds it with `--locked`, verifies its dependency tree and compares its
results with a real CLI invocation on the same program.

The `TestHost` here is a bounded in-memory host for this consumer, not a
published product host: it grants no external effect, and cannot be described
as a sandbox or as equivalent to the CLI's `pure` mode.

Run:

```bash
cargo run --locked --manifest-path tests/embedding/Cargo.toml
```
