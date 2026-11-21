# Evidence: `std:env` and the `env.get` grant

Date: 2026-11-21 (Asia/Shanghai)
Source baseline: `ae69391bc0e51e0b87f3ba15a3679111c40a1231`
Repository: `marqdo-cli`
Decision record: [ADR-0043](../adr/0043-env-resource-operation.md)
RFC item: P2 environment product API

## Implemented

`resource.request("env.get", [name])` is a project-mode operation:

- exactly one Text name; wrong arity/type is fatal;
- `Capabilities::env` is an exact-name list; `--allow-env NAME` adds one
  grant; un-granted names return `permission.denied` before `std::env::var`;
- missing names are recoverable `env.not_found`; non-UTF-8 values are
  `env.not_unicode`;
- no `env.list`, `env.set` or process execution.

`stdlib/env.mq.md` is a thin `.mq.md` facade: `读取` returns the Result and
`存在` maps `env.not_found` to `{ok:true, value:false}`.

## Tests

`stdlib/tests/env.mq.md` runs through `std:env` with
`--allow-env MARQDO_TEST_ENV --allow-env MARQDO_MISSING_ENV`; the harness sets
`MARQDO_TEST_ENV=env-value`. `crates/marqdo-host/tests/resource_streams.rs`
pins exact grants, pure-mode refusal, `env.not_found` and arity.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                476 passed, 0 failed, 2 ignored
cargo test -p marqdo-host --test resource_streams        22 passed
cargo test -p marqdo-cli --test stdlib_product           13 passed
python3 tools/pack-stdlib.py                            PASS
```

## Not covered

No environment enumeration, mutation, expansion, per-call delegation or
process execution. `std:process` remains open and needs its own ADR and OS
sandbox story.
