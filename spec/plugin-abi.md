# Marqdo Plugin ABI v1

Status: **frozen ABI v1.0** (`MQ_ABI_MAJOR=1`, `MQ_ABI_MINOR=0`). The canonical
C header is `include/marqdo_plugin_abi_v1.h`; the Rust mirrors are in
`crates/marqdo-plugin-abi/src/abi.rs`; the plugin-side SDK is
`crates/marqdo-plugin-sdk`.

> **A C ABI is not a sandbox.** A native plugin is loaded into the host
> process. It can read host memory, call arbitrary syscalls, bypass the
> capability model and crash the host. In-process loading requires an explicit
> trust decision. `kind: worker` runs the same native library in an isolated
> child process with length-prefixed JSON frames; that isolates crashes and
> enforces a handshake/call timeout, but a plain child process is **not** an OS
> sandbox. `strict` still refuses without a real isolation adapter.

## 1. Independent versions

Three versions exist and are never conflated:

| Version | Current | Where |
|---|---|---|
| Marqdo language/semantics | M0.5, `spec/*` | `docs/DEVELOPMENT_BLUEPRINT.md`, specs |
| Plugin ABI major.minor | `1.0` | `MQ_ABI_MAJOR`/`MQ_ABI_MINOR`, `marqdo_plugin_abi::ABI_MAJOR/ABI_MINOR` |
| Plugin package SemVer | e.g. `0.1.0` | `plugin.yaml` `plugin.version` |

The JSON wire protocol has its own `wire_version = 1`
(`spec/value-wire.md`); it is not the ABI major version.

## 2. Stable-layout types

The header defines only fixed-width integers, `size_t`-based length+pointer
buffers, opaque host-owned handles and `extern "C"` function pointers. Rust
`String`/`Vec`/trait objects/`Result`/panics, C++ exceptions, Go objects and
raw host pointers never cross the boundary.

On a 64-bit target (`include/marqdo_plugin_abi_v1.h` static assertions and
`crates/marqdo-plugin-abi/src/abi.rs` compile-time assertions):

| Type | Size | Align |
|---|---|---|
| `MqBytes` | 16 | 8 |
| `MqOwnedBytes` | 16 | 8 |
| `MqPluginApi` | 32 | 8 |
| `MqHostApi` | 56 | 8 |
| `MqPluginInvoke` (fn ptr) | 8 | — |

On a 32-bit target the Rust side asserts `MqBytes`/`MqOwnedBytes` = 8,
`MqPluginApi` = 20 and `MqHostApi` = 28. The layout uses `size_t`, so an
artifact is only loadable by a host built for the **same target triple, pointer
width and byte order**. `(NULL, 0)` is the only empty buffer; a non-empty
buffer with a NULL pointer is a protocol error.

### 2.1 Ownership and lifetimes

| Buffer | Producer | Lifetime / free rule |
|---|---|---|
| `args_wire`, `export_name`, `signature_wire`, capability `request_wire`, `plugin_metadata_wire` (as seen by host) | caller passing it | borrowed for the duration of the call; consumer must copy and must not retain or free |
| `result_wire`, `error_wire`, capability `reply_wire` | the producing side | must be allocated with `host->alloc(host->host_data, size)` and is released with `host->free(host->host_data, data)` by the consumer after copying; never `malloc`/`new`/GC memory |
| `plugin_metadata_wire` (as returned by plugin) | plugin static storage | valid only during `mq_plugin_entry_v1`; host copies it immediately |

`MqCallContext` is opaque, host-owned and valid only for the invocation it was
minted for. Plugins must not dereference, cache or copy it.

### 2.2 Size limits (enforced by host / SDK)

| Constant | Bytes |
|---|---|
| `MQ_MAX_EXPORT_NAME_BYTES` | 256 |
| `MQ_MAX_SIGNATURE_WIRE_BYTES` | 4096 |
| `MQ_MAX_METADATA_WIRE_BYTES` | 65536 |
| `MQ_MAX_REQUEST_WIRE_BYTES` | 8 MiB - 4096 (8,384,512) |
| `MQ_MAX_RESULT_WIRE_BYTES` | 8 MiB - 4096 (8,384,512) |
| `MQ_MAX_ERROR_WIRE_BYTES` | 1 MiB (1,048,576) |
| `MQ_MAX_CAPABILITY_REPLY_BYTES` | 8 MiB - 4096 |

Wire JSON decoding additionally applies the default `WireLimits` from
`spec/value-wire.md` (depth 64, 1,000,000 items, 16 MiB strings). Oversized
payloads are rejected, never truncated and reparsed.

## 3. Exported entry point and tables

Required symbol: `mq_plugin_entry_v1` (`MQ_PLUGIN_ENTRY_SYMBOL`):

```c
int32_t mq_plugin_entry_v1(const MqHostApi *host, MqPluginApi *plugin);
```

Called exactly once, by the loading thread:

* the host supplies a valid `MqHostApi` table and a zeroed `MqPluginApi` whose
  `struct_size = sizeof(MqPluginApi)`;
* the plugin must validate the host table, fill `plugin.abi_major`,
  `plugin.abi_minor`, `plugin.plugin_metadata_wire` and register every export
  via `host->register_function`;
* it returns `MQ_OK` only after all exports are registered; on non-zero the
  host ignores the plugin table and fails the load.

`MQ_OK` and error codes (stable integers):

| Code | Name | Meaning |
|---|---|---|
| 0 | `MQ_OK` | success |
| 1 | `MQ_ERR_INVALID_ARGUMENT` | NULL pointer, bad pointer/length pair |
| 2 | `MQ_ERR_ABI_MISMATCH` | ABI major mismatch or incompatible `struct_size` |
| 3 | `MQ_ERR_REGISTRATION` | an export could not be registered |
| 4 | `MQ_ERR_UNSUPPORTED` | operation/type/capability unsupported |
| 5 | `MQ_ERR_OUT_OF_MEMORY` | host allocation failed |
| 6 | `MQ_ERR_BAD_WIRE` | payload is not valid `wire_version` 1 JSON |
| 7 | `MQ_ERR_INTERNAL` | unexpected failure, including caught panic |
| 8 | `MQ_ERR_CAPABILITY` | capability request denied/failed |

### 3.1 Version negotiation

Host side (`negotiate` in `crates/marqdo-plugin-abi/src/abi.rs`):

* `plugin.abi_major != 1` → load fails with `abi_major_mismatch`;
* `plugin.abi_minor` greater than the host minor is accepted but **clamped** to
  the host minor (currently 0); only features defined at that minor may be
  used;
* the plugin must reject a host table with `host == NULL`,
  `host->struct_size < sizeof(MqHostApi)`, `host->abi_major != 1` or NULL
  alloc/free/register_function/request_capability pointers.

`struct_size` is the extension mechanism: fields may only be appended in a
later ABI minor, never reordered or resized; a consumer only reads a field when
`struct_size` covers it.

### 3.2 Threading and lifetime

v1 is single-threaded and synchronous: entry is called once; invocations and
host callbacks for one plugin instance run on that thread with at most one call
in flight. A plugin must not start threads that call back into `MqHostApi`.
v1 has no shutdown callback and no hot unload/reload; the `LoadedPlugin`
wrapper unloads only when dropped, and the docs require one instance per
artifact path (ABI v1 plugins may keep the host table in a process-global).
No panic/exception/longjmp may cross the boundary; the SDK catches Rust panics
and returns `MQ_ERR_INTERNAL` with a structured error envelope.

## 4. `plugin.yaml`

```yaml
plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: x86_64-unknown-linux-gnu
      file: solver.so
      sha256: "<64 lowercase hex>"
```

Validation (`crates/marqdo-plugin-abi/src/manifest.rs::PluginManifest::validate`):

* `name`: dotted segments, each starting with a lowercase ASCII letter and
  containing only lowercase letters, digits, `-`/`_`; ≤ 256 bytes; package
  names starting with `marqdo.` or `builtin.` are reserved;
* `version`: semver-ish `major.minor.patch` with optional `-prerelease` and
  `+build`; numeric components have no leading zeros; not a SemVer range
  resolver;
* `abi`: caret-style requirement `^1`, `^1.0`, `1` or `1.0`. Caret semantics:
  `^M.m` means `>=M.m, <M+1.0`; the `0.x` special case pins the minor. It must
  be compatible with the host's `1.0`;
* `kind`: `native`, `worker` or `wasm`. `native` loads in-process only with
  explicit trust; `worker` loads in a child process without
  `--trust-native-plugin`; `wasm` is explicitly unsupported;
* `exports`: at least one, at most 256, unique, namespaced (must contain `.`),
  no reserved prefix (`marqdo.`, `builtin.`, `core.`, `std.`, `host.`),
  non-empty signature ≤ 4096 bytes, signature starts with `(` and contains
  `->`, no control characters;
* `requires`: at most 64, unique names from the whitelist `log`,
  `math.compute`, `fs.read`, `fs.write`, `net.request`, `clock`, `random`,
  `artifact.record`; this is a declaration, not a grant;
* `artifacts`: 1–32, one per target triple; target parses as a 3–4 component
  triple (lowercase, alphanumeric/underscore components); `file` is a
  non-empty relative path ≤ 256 bytes with no absolute/`..` escape; `sha256` is
  exactly 64 hex characters.

Manifest parsing/validation never executes plugin code and performs no
filesystem access (artifact verification is a separate step).

## 5. Artifact selection and verification

* `HostTarget::current()` derives a conventional triple from
  `std::env::consts` (`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
  `x86_64-pc-windows-msvc`, ...).
* `resolve_artifact` requires **exact triple equality**; pointer width and byte
  order follow from the triple. No fallback to another architecture.
* `verify_artifact` resolves the path below the manifest directory and compares
  the file's SHA-256 with the manifest (case-insensitive hex). A missing file
  is `missing_artifact`; a digest mismatch is `hash_mismatch`.
* Verification proves the artifact is the file described by the manifest; it
  proves nothing about trust or authorization.

## 6. Loader lifecycle (host side)

`crates/marqdo-plugin-abi/src/loader.rs::LoadedPlugin::open_with`:

1. `manifest.validate()`;
2. require `kind == native` (otherwise `plugin_kind_not_loadable_in_process`);
3. resolve artifact for the host target and verify its sha256;
4. `libloading::Library::new` (a trust decision, not a sandbox);
5. build the host API table and `HostState`, initialize `MqPluginApi`;
6. call `mq_plugin_entry_v1` once; non-zero → structured load error;
7. require `plugin_api.struct_size >= sizeof(MqPluginApi)`;
8. negotiate ABI major/minor;
9. copy `plugin_metadata_wire` and require metadata `name`/`version` to match
   the manifest exactly;
10. take registered exports and cross-check against the manifest: duplicate
    registrations, missing declared exports, signature mismatches and
    undeclared exports all fail the load;
11. keep the library mapped for the lifetime of `LoadedPlugin`.

Invocation (`LoadedPlugin::invoke`): validate the request envelope, check the
request size, find the registered export, mint a `MqCallContext`, call the
plugin's `MqPluginInvoke`, copy and free both output buffers, then enforce the
envelope contract:

* success (`MQ_OK`): `error_wire` must be empty; `result_wire` must be a
  non-empty `wire_version` 1 success envelope; the typed value is optional
  (absent `value` means `Unit`);
* failure: `error_wire` must hold a `wire_version` 1 error envelope; a missing
  or invalid error envelope still fails the call (structured
  `plugin_call_failed`).

Capability requests from the plugin go through
`MqHostApi::request_capability` into the host's `CapabilityHandler`, on the
same thread, one at a time. The handler receives the capability name and
request bytes and returns reply bytes or an error. When no handler is
installed (the CLI's case) **every request is denied** (`MQ_ERR_CAPABILITY`).

## 7. Wire envelopes

All payloads are UTF-8 JSON with `wire_version = 1`. Values inside `args` and
`value` use the type-tagged encoding of `spec/value-wire.md`.

Request:

```json
{
  "wire_version": 1,
  "kind": "request",
  "function": "solver.add",
  "args": {
    "a": {"type": "int", "decimal": "20"},
    "b": {"type": "int", "decimal": "22"}
  },
  "context": {"trace_id": "optional", "capability_handle": "opaque-and-scoped"}
}
```

Response (success):

```json
{"wire_version":1,"kind":"response","value":{"type":"int","decimal":"42"}}
```

A success response with neither `value` nor `error` means `Unit`. A response
with both is rejected (`ambiguous_response`). Error:

```json
{"wire_version":1,"kind":"response","error":{
  "kind":"invoke","code":"missing_argument",
  "message":"required argument `a` is missing","recoverable":true
}}
```

Envelope limits: at most `MAX_ARGS = 1024` request arguments; request and
response ≤ 8 MiB - 4096, error ≤ 1 MiB. `context` carries no secrets and no
reconstructable OS handles. `signature_wire` is
`{"signature":"<signature string>"}`; `plugin_metadata_wire` is
`{"name":"<package name>","version":"<semver>"}` (extra fields ignored).

## 8. Error model

Host-side errors are `PluginError { kind, code, message, detail?, recoverable }`
with stable `kind` values: `manifest`, `abi`, `artifact`, `load`, `wire`,
`capability`, `invoke`, `unsupported`. Examples: `abi_major_mismatch`,
`no_matching_artifact`, `hash_mismatch`, `missing_entry_symbol`,
`metadata_name_mismatch`, `export_signature_mismatch`,
`unsupported_wire_version`. `message`/`detail` must never contain secrets.
They are serializable and are used as the wire error body.

When a plugin is used from Marqdo code, host errors become structured
`RuntimeError`s (`crates/marqdo-host/src/host.rs::map_plugin_error`):

| Plugin error kind | Runtime code |
|---|---|
| `abi` | `plugin.abi_mismatch` |
| `capability` | `permission.denied` |
| `wire` | `runtime.type` |
| `unsupported` | `kernel.unavailable` |
| `manifest`/`artifact`/`load`/`invoke` | `plugin.failure` |

## 9. Using a plugin from `.mq.md`

This build supports explicitly trusted **in-process native** plugins only.

```bash
# 1. Build the C example and refresh its sha256 in plugin.yaml
./examples/plugin-demo/build.sh

# 2. Inspect without loading
marqdo plugin inspect examples/plugin-demo

# 3. Load with an explicit trust decision and a capability grant, then run
marqdo run examples/plugin-demo/plugin-call.mq.md \
  --plugin examples/plugin-demo \
  --trust-native-plugin \
  --allow-plugin demo.solver
```

Flags (`run` and `call`): `--plugin <manifest-or-dir>` (repeatable),
`--trust-native-plugin`, `--allow-plugin <package-name>`. `--allow-plugin`
matches the manifest `plugin.name` (for example `demo.solver`), not the call
namespace (`solver`).

Document syntax (`examples/plugin-demo/plugin-call.mq.md`):

```markdown
## @exec main

`结果 = solver.add(a=20, b=22)`
`print(结果)`
`return 结果`
```

Semantics:

* every manifest export is registered by its **complete dotted name**
  (`solver.add`, `graph.search`); there is no first-prefix/namespace-only
  registration. A manifest is loaded atomically: a duplicate export from
  another plugin rejects the whole load rather than overwriting (ADR-0010);
* `check --plugin <manifest>` reads manifests and validates export signatures
  statically without loading a shared library. `run/call --plugin` performs the
  same static validation before execution; a bad arity/parameter name is
  `call.arity`, not a plugin failure;
* calls accept **named arguments only**; positional arguments are a runtime
  type error ("plugin calls require named arguments");
* each call becomes `HostOp::PluginInvoke` with the complete export name and a
  typed wire request; the plugin's typed response becomes a Marqdo value
  (`Unit` when absent);
* the capability grant is re-checked on every call, not only at load;
* plugin host-capability requests go through `CapabilityBroker`: core
  capabilities are checked against effective grants; vendor capabilities need a
  trusted policy entry for the exact name and otherwise fail with
  `capability.unavailable` (ADR-0011);
* `--mode strict` refuses to run before any plugin is loaded; `--mode pure`
  cannot grant `--allow-plugin`, so loading fails with `permission.denied`.

Trust boundary: `--trust-native-plugin` is an explicit unsafe decision; the
process shares its address space with the plugin. `kind: worker` avoids sharing
the address space: the core binary is re-invoked as `__plugin-worker`, loads
one native library in the child, reports its complete export table, and serves
length-prefixed invoke frames. Handshake and calls have timeouts; a crash/EOF
becomes `plugin.crashed`/`plugin.timeout` in the host. This is crash isolation,
not an OS sandbox: the worker still runs as the user. Wasm, hot unload and a
full plugin capability broker for worker requests remain future work.

### 9.1 Worker frames

Frames are a 4-byte big-endian length followed by one JSON object (max 8 MiB).
A zero/oversized length, partial header, truncated payload or invalid JSON is a
protocol error; clean EOF at a frame boundary is not. The host marks the worker
dead after a transport failure and does not reuse it.
Worker → host: `{"kind":"ready","exports":[...]}`. Host → worker:
`{"kind":"invoke","request":<wire request>}` and `{"kind":"shutdown"}`.
Worker → host responses are `{"kind":"response","response":<wire response>}` or
`{"kind":"error","code":...,"message":...}`. The worker validates that the
declared manifest exports exactly match the library's registrations.

While an invoke is in flight, the worker may also request host capabilities
(ADR-0018):

```text
worker → host  {"kind":"capability_request","id":7,"capability":"fs.read",
                "request":{"path":"data/allowed.txt"}}
host → worker  {"kind":"capability_reply","id":7,"data":"<utf-8 text>"}
host → worker  {"kind":"capability_error","id":7,"code":"permission.denied",
                "message":"..."}
```

The id must match and is request-scoped; requests carry no trusted identity
(the host binds package/session from the spawned worker). Non-JSON requests,
unknown/mismatched ids, non-text replies and other frame kinds are protocol
errors. The invoke deadline covers the whole exchange. In this build only
granted `fs.read` executes; other capabilities return `capability.unavailable`.
At most 64 capability requests per invocation and 64 KiB per serialized
request; exceeding either is `capability.budget_exceeded`/`request_too_large`.
Both the worker handler and the host enforce the limits (the host kills the
worker on a violation). These frames do not change the C ABI.

## 10. SDK usage (`crates/marqdo-plugin-sdk`)

The SDK hides the raw C layout and exports the entry symbol with
`mq_plugin_entry!`. A complete Rust plugin crate:

```rust
use marqdo_plugin_sdk::{mq_plugin_entry, CallContext, PluginError, Value};

fn add(_ctx: &mut CallContext, args: &[u8]) -> Result<Vec<u8>, PluginError> {
    let request = marqdo_plugin_sdk::wire::decode_request(args)?;
    let values = request.typed_args()?;
    let a = marqdo_plugin_sdk::wire::int_arg(&values, "a")?;
    let b = marqdo_plugin_sdk::wire::int_arg(&values, "b")?;
    marqdo_plugin_sdk::wire::success(Some(Value::Int((a + b).into())))
}

mq_plugin_entry! {
    name: "demo.solver",
    version: "0.1.0",
    exports: {
        "solver.add" => "($a: Int, $b: Int) -> Int" => add,
    }
}
```

The macro validates the host table, reports the plugin ABI version and metadata,
registers every export and returns `MQ_OK` only if all registrations succeed.
Handlers receive a `&mut CallContext`; use
`CallContext::capability_request(name, request_bytes)` for an already-authorized
host capability (the CLI grants none today), and
`CallContext::log(level, message)` as a convenience. Panics are caught at the
ABI boundary and converted to a structured error. The SDK depends only on
`marqdo-plugin-abi`, never on host-only crates or the interpreter.

## 11. C demo (`examples/plugin-demo`)

`examples/plugin-demo/solver.c` is a dependency-free C implementation of one
export (`solver.add`, `Int + Int`) using a minimal JSON scanner; it validates
the host table, reports metadata and registers the export. `build.sh` compiles
it (`cc -shared -fPIC -I include`) and rewrites the `sha256` in `plugin.yaml`
(the checked-in manifest uses a zero placeholder until you build). The
`examples/plugin-demo/README.md` walks through the security model, build,
inspect and call flow.

The integration test `crates/marqdo-plugin-abi/tests/loader_unix.rs` compiles
this exact `solver.c` with `cc`, loads it with `libloading`, round-trips a wire
request, and rebuilds it with `-DMQ_SOLVER_REPORT_ABI_MAJOR=2` to prove that a
wrong ABI major fails the load. It also covers a missing entry symbol, missing
artifact, hash mismatch, malformed manifest and target mismatch. The tests skip
when no C compiler is available.

## 12. Not implemented / explicit limitations

| Item | Status |
|---|---|
| Isolated plugin worker transport (length-prefixed IPC) | Implemented for crash isolation with handshake/call timeouts; a child process is not an OS sandbox. |
| Wasm Component + WIT | Not implemented; `kind: wasm` is rejected explicitly. |
| Hot unload/reload | Not supported by ABI v1; the worker is killed on unload. |
| Plugin installation by package manager | Not implemented; `marqdo.yaml` `plugins:` is declarative only. |
| Vendor capability handlers | The broker and vendor-name validation exist; no concrete vendor handler is shipped in this build. |
| Static validation without an explicit manifest | `check` needs `--plugin <path>`; descriptors are not discovered from the network. |
| Positional plugin arguments | Not supported; named only. |
| Thread-safe / concurrent invocation | Not supported (single-threaded v1 contract). |
| Old `marqdo_abi.h` v2 compatibility | Not implemented and not claimed; a separate adapter would be required. |
