# Plugins, foreign kernels, resource providers and lock v4

Three independent extension kinds share one principle: **selection is not
authorization**. A manifest makes a name visible; the user still grants the
exact name or flag at run time. None of these extensions may add a language
keyword or a built-in function to the core.

| Kind | Descriptor | Execution form | Authorization |
|---|---|---|---|
| Callable plugin | `plugin.yaml` (Plugin ABI v1) | native in-process **only with trust**, or worker child process | `--plugin PATH` + `--allow-plugin NAME` (+ `--trust-native-plugin` for native) |
| Foreign kernel | `kernel.json` (Foreign wire v1) | one-shot child process | `--kernel PATH` + `--allow-foreign` |
| Resource provider | `provider.json` (Provider manifest v2) | `marqdo-provider-worker` + C ABI v1 library in a child process | `--provider PATH` + `--allow-provider NAME` |

`pure` and `strict` refuse all three. Worker/child-process transports are crash
isolation, **not** OS sandboxes; a C ABI is not a sandbox.

## 1. Callable plugins

```yaml
plugin:
  name: demo.solver
  version: 0.1.0             # plugin package version (independent axis)
  abi: "^1.0"                # Plugin ABI major 1
  kind: worker               # native | worker; wasm is rejected
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: x86_64-unknown-linux-gnu
      file: libsolver.so
      sha256: "<64 lowercase hex of the actual artifact>"
```

```bash
marqdo plugin inspect path/to/plugin.yaml        # static; never loads the library
marqdo check index.mq.md --plugin path/to/plugin.yaml
marqdo run index.mq.md --plugin path/to/plugin.yaml --allow-plugin demo.solver
```

- `check --plugin` validates the manifest and export signatures without loading
  the artifact; `run` verifies target triple, file SHA-256, grant, and then
  reconciles loaded exports/signatures/owners/digests one-to-one with the
  manifest (`ext.plugin_binding_mismatch` on any difference).
- Calls use named arguments; positional arguments are normalised to manifest
  order but ABI v1 configuration defaults are rejected.
- `kind: native` shares the host address space and can crash or compromise it;
  it requires the explicit unsafe `--trust-native-plugin`. `kind: worker` runs
  the same library in a child process with framed JSON and timeouts; the C ABI
  header/SDK is `include/marqdo_plugin_abi_v1.h` and `crates/marqdo-plugin-sdk`.
- A runnable host-built example is `examples/plugin-demo/` (native) and its
  generated manifest; the release archive includes the built artifact.
- Normative contract: [`../../spec/plugin-abi.md`](../../spec/plugin-abi.md),
  [`../../spec/callable-resolution.md`](../../spec/callable-resolution.md).

## 2. Foreign kernels

```json
{
  "provider_id": "mocklang",
  "languages": ["mocklang", "mock"],
  "wire_version": 1,
  "entrypoint": "./mocklang",
  "permissions_requested": [],
  "digest": "<sha256 of the executable/environment artifact>",
  "environment_id": "mocklang-env-1"
}
```

```bash
marqdo run demo.mq.md --kernel path/to/kernel.json --allow-foreign
```

Fenced `Code` and `Formula` values are inert; the program must call
`foreign.run`/`foreign.call` explicitly. The host refuses unauthorized
execution, digest mismatch, unexpected exit, timeout and an incompatible wire
version. Add a language tag by writing a provider, not by adding an
`if language == …` branch to the core. Example:
`examples/kernel-mock/`. Normative contract:
[`../../spec/foreign.md`](../../spec/foreign.md).

## 3. Resource providers v2

```json
{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [{
    "target": "x86_64-unknown-linux-gnu",
    "path": "lib/libsensor.so",
    "sha256": "<64 lowercase hex>"
  }],
  "permissions_requested": []
}
```

```bash
marqdo run demo.mq.md \
  --provider path/to/provider.json \
  --allow-provider demo.sensor
```

The program still uses the ordinary language surface
`resource.open("demo.sensor", "unit-1", ["read"])` plus
`resource.read/write/close` and `Memory` for bytes. There is no provider-specific
keyword or built-in. The selected provider is verified (schema version, target,
artifact SHA-256, duplicate kind, grant) before the sibling
`marqdo-provider-worker` starts; a missing helper is
`ext.worker_unavailable` and never falls back to `PATH` or in-process loading.
Linux x86_64 only for this beta; not an OS sandbox. Runnable example:
`examples/provider-demo/` (build with its `build.sh`; the tracked manifest has a
zero digest template). Normative contract:
[`../../spec/resource-provider-v2.md`](../../spec/resource-provider-v2.md).

## 4. Reproducible selection: `extensions` + lock v4

Declare activation sources in the project manifest:

```yaml
extensions:
  plugins:
    - from: ./adapter/plugin.yaml
  kernels: []
  providers:
    - from: "@vendor/device/provider.json"
```

`from` is exactly `./relative/path` inside the project or
`@alias/relative/path` inside a declared dependency. Absolute/rooted paths,
URLs, globs, `..`, symlink escapes and duplicate entries are rejected; the
alias must exist in `dependencies`.

```bash
marqdo lock
marqdo inspect index.mq.md --extensions --json   # identities + lock status; no execution
```

`lock` records manifest and artifact identities (including the selected target)
in lock v4. `check`/`sync` compare the declaration to the lock
(`ext.extension_lock_mismatch`), and `run` re-verifies the real artifact bytes
before starting a worker. Lock v3 gets an explicit `marqdo lock` migration error.
Normative contract:
[`../../spec/extension-selection.md`](../../spec/extension-selection.md).

## 5. Frozen-binary test matrix

Your external repository must pass with an unchanged CLI binary:

- positive: pinned pure library; plugin under correct grant; kernel under
  `--allow-foreign`; provider under `--allow-provider`; extension selection
  locked and re-verified;
- negative: wrong export/arity, duplicate export, missing/edited artifact,
  digest or target mismatch, missing helper, unauthorized grant, wrong lock
  version, unknown schema major, worker crash/timeout — all must fail with
  their documented diagnostics and no silent fallback.

The reference suite is
[`marqdo-extension-conformance`](https://git.cflmy.cn/cflmy/marqdo-extension-conformance);
the core's evidence generator is `tools/ci/verify_extension_contracts.py`.
