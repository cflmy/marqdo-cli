# sample-knowledge

A tiny independent Tool Protocol v1 consumer used to prove that arbitrary
top-level tool names dispatch through the registry, not through a hard-coded
CLI match arm. It is a test fixture, not part of a View/Knowledge/Proof
product.

Build and install (any OS with a Rust toolchain):

```sh
rustc --edition 2021 examples/tool-samples/sample-knowledge/src/main.rs \
  -o /tmp/marqdo-tool-sample-knowledge/bin/sample-knowledge
cp examples/tool-samples/sample-knowledge/tool.json /tmp/marqdo-tool-sample-knowledge/
MARQDO_TOOL_HOME=/tmp/marqdo-tools marqdo tool install /tmp/marqdo-tool-sample-knowledge/tool.json
```

See `crates/cli/tests/tool_protocol.rs` for the automated Analysis/Trace
consumer test.
