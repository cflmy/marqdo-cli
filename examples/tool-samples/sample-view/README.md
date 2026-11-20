# sample-view

A tiny independent Tool Protocol v1 consumer used to prove that arbitrary
top-level tool names dispatch through the registry, not through a hard-coded
CLI match arm. It is a test fixture, not part of a View/Knowledge/Proof
product.

Build and install (any OS with a Rust toolchain):

```sh
mkdir -p /tmp/marqdo-tool-sample-view/bin
rustc --edition 2021 examples/tool-samples/sample-view/src/main.rs \
  -o /tmp/marqdo-tool-sample-view/bin/sample-view
cp examples/tool-samples/sample-view/tool.json /tmp/marqdo-tool-sample-view/
MARQDO_TOOL_HOME=/tmp/marqdo-tools marqdo tool install /tmp/marqdo-tool-sample-view/tool.json
```

See `crates/cli/tests/tool_protocol.rs` for the automated Analysis/Trace
consumer test.
