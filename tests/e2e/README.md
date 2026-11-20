# End-to-end lanes

The executable end-to-end lanes live next to the binary:

| Lane | Test | What it runs |
|---|---|---|
| interactive stdio | `crates/cli/tests/resource_streams.rs` | piped stdin, separate stdout/stderr, EOF, partial reads/writes |
| print compatibility | `crates/cli/tests/print_routing.rs` | `打印/print` through the bundled `std:io` path |
| HTTP over TCP | `crates/cli/tests/http_over_tcp.rs` | HTTP/1.1 exchange over an authorized IP-literal TCP stream |
| whole stdlib | `crates/cli/tests/stdlib_product.rs` | every `stdlib/tests/*.mq.md` through `std:` imports |
| CLI contract | `crates/cli/tests/integration_cli.rs` | `run`/`call`/`check`/`inspect`/`package`/`tool` |

A fixture alone is not an end-to-end lane. These tests spawn the frozen CLI
binary; update `docs/STATUS.md` with the executed command, not with a plan.
