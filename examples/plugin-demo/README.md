# Marqdo plugin ABI v1 demo (C)

This example shows how a third-party C developer implements a Marqdo native
plugin against the frozen, versioned C ABI v1 in
[`include/marqdo_plugin_abi_v1.h`](../../include/marqdo_plugin_abi_v1.h), with
no Marqdo source access and no Rust dependency.

It implements one export:

| Export       | Signature                          | Behaviour |
| ------------ | ---------------------------------- | --------- |
| `solver.add` | `($a: Int, $b: Int) -> Int`        | Returns `a + b` using the wire_version 1 JSON envelope |

## Security model (read this first)

- **A C ABI is not a sandbox.** A loaded native plugin runs in the host
  process and can read memory, crash the process or bypass the capability
  model entirely.
- **Installing is not loading.** Placing the package in a plugin directory
  grants nothing.
- **Loading is not permission.** Loading requires an explicit host/user trust
  decision for in-process native code.
- **Declaring is not granting.** `requires: [math.compute]` only states what
  the plugin wants; the host authorization layer decides whether the
  capability is granted. `request_capability` can never escalate.
- Default third-party execution must use the isolated worker path. This demo
  is for trusted, in-process development and ABI conformance testing.

## Build

From the repository root (gcc/clang):

```sh
cc -std=c11 -O2 -shared -fPIC -I include examples/plugin-demo/solver.c \
   -o examples/plugin-demo/solver.so
```

The artifact is target-specific. Build it for the same target triple, pointer
width and byte order as the host; 32-bit and 64-bit artifacts are never
interchangeable. Update `artifacts[0].target` and `sha256` in
[`plugin.yaml`](plugin.yaml) to match what you built:

```sh
sha256sum examples/plugin-demo/build/solver.so
```

## Inspect

Manifest validation and artifact verification are implemented by
`crates/marqdo-plugin-abi` and exposed through `marqdo plugin inspect` (which
never loads plugin code):

```sh
./examples/plugin-demo/build.sh   # writes build/solver.<dylib> + manifest
marqdo plugin inspect examples/plugin-demo/build
```

A typical inspection flow is:

1. parse `plugin.yaml`,
2. validate name, semver-ish version, `^1.0` ABI requirement, kind, exports,
   capability names and artifact declarations,
3. resolve the artifact for the host target triple,
4. verify the sha256 against the manifest,
5. only then (with explicit trust) load and call
   `mq_plugin_entry_v1`.

The entry point must:

- check `host->struct_size` and `host->abi_major == MQ_ABI_MAJOR`
  (`MQ_ERR_ABI_MISMATCH` otherwise),
- report `plugin->abi_major`/`abi_minor` and `plugin_metadata_wire`,
- register every manifest-declared export via `host->register_function`.

The ABI is single-threaded: the entry point is called once by the loading
thread, invocations and host callbacks happen on that same thread, one call at
a time. The host does not support hot-unloading in v1.

## Load and call

The host-side loader (`marqdo_plugin_abi::LoadedPlugin`) performs the full
lifecycle and invokes `solver.add` with a wire request:

```text
wire request : {"wire_version":1,"kind":"request","function":"solver.add",
                "args":{"a":{"type":"int","decimal":"20"},
                        "b":{"type":"int","decimal":"22"}},
                "context":{}}
wire response: {"wire_version":1,"kind":"response",
                "value":{"type":"int","decimal":"42"}}
```

The integration test
`crates/marqdo-plugin-abi/tests/loader_unix.rs` compiles this exact
`solver.c` with `cc -shared -fPIC`, loads it with `libloading`, and round-trips
the request. It duplicates the compilation with
`-DMQ_SOLVER_REPORT_ABI_MAJOR=2` to prove that a wrong ABI major fails the
load.

## Buffer rules in one paragraph

The host owns `alloc`/`free`. Input buffers (`args_wire`, export names,
signatures, metadata) are borrowed only for the duration of the call; the
plugin must copy and never retain or free them. Output buffers
(`result_wire`, `error_wire`, capability replies) must be allocated with
`host->alloc` and are then freed by the host after copying. Zero-length
buffers are `{NULL, 0}`. On success fill `result_wire` and leave
`error_wire` empty; on failure fill `error_wire` with a JSON error envelope
and leave `result_wire` empty. Size limits are defined in the header (16 MiB
requests, 64 MiB results, 1 MiB errors).

## What the demo deliberately does not do

The JSON scanner in `solver.c` is intentionally minimal (no escapes, `i64`
only) to keep the example dependency-free. A real plugin should use a proper
JSON library and its language's SDK bindings (the Rust SDK is
`crates/marqdo-plugin-sdk`). The demo also does not request capabilities; see
`CallContext::capability_request` in the SDK for that path.


## Call from a Marqdo document (implemented)

Once the host has made the explicit in-process trust decision, plugin exports
are callable through the namespace that prefixes their export names. Plugin
calls take **named arguments only** in this build:

```markdown
## main

**结果 = solver.add(a=20, b=22)**

> 打印(结果)
```

```sh
marqdo run examples/plugin-demo/plugin-call.mq.md \
  --plugin examples/plugin-demo \
  --trust-native-plugin \
  --allow-plugin demo.solver
# 42
```

- `--plugin <path>` loads one manifest/artifact (repeatable).
- `--allow-plugin demo.solver` grants the plugin capability; without it the
  call fails with `permission.denied`.
- `--trust-native-plugin` acknowledges that in-process native code shares the
  host address space and is not sandboxed. This build has no isolated worker
  transport, and `--mode strict` refuses to run at all.
- Plugin failures become structured runtime errors: `plugin.abi_mismatch`,
  `plugin.failure`, `symbol.unknown` (namespace not loaded), or
  `permission.denied`.
