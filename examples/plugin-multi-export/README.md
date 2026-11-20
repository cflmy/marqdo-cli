# Multi-export plugin example

One native library exports two complete names: `solver.add` and `solver.neg`.
Both are registered and callable; there is no "first prefix wins" namespace.

```sh
./examples/plugin-multi-export/build.sh   # writes build/ for this host
marqdo check examples/plugin-multi-export/plugin-call.mq.md \
  --plugin examples/plugin-multi-export/build
marqdo run examples/plugin-multi-export/plugin-call.mq.md \
  --plugin examples/plugin-multi-export/build \
  --trust-native-plugin --allow-plugin demo.solver
```

`check --plugin` validates both signatures without loading the library. The
runtime still requires the explicit in-process trust decision; a C ABI is not a
sandbox, and an isolated worker transport is not part of this build.
