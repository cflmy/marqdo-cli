# Marqdo Security Model (implemented status)

Status: **describes the M0.5 implementation, including honest gaps.** This is
not a security audit; it documents what the code actually enforces.

Normative implementation:

| Concern | Code |
|---|---|
| Sandbox modes, strict refusal | `crates/marqdo-sandbox/src/lib.rs` |
| Capability sets and checks | `crates/marqdo-host/src/capability.rs` |
| Host dispatch and per-op checks | `crates/marqdo-host/src/host.rs` |
| Project-rooted filesystem | `crates/marqdo-host/src/fs.rs` |
| Budgets | `crates/marqdo-host/src/budget.rs` |
| Effect tracing | `crates/marqdo-host/src/trace.rs` |
| Math/foreign adapters | `crates/marqdo-host/src/kernels.rs` |
| Foreign child processes | `crates/marqdo-foreign/src/lib.rs` |
| Plugin loading / in-process plugin calls | `crates/marqdo-host/src/host.rs` (`load_plugin`, `op_plugin_invoke`), `crates/marqdo-plugin-abi/src/loader.rs` |
| CLI flags and capability construction | `crates/cli/src/main.rs`, `crates/cli/src/commands.rs` |

## 1. Modes

`SandboxMode` has three values (`pure`, `project`, `strict`), parsed by
`SandboxMode::parse` and selectable on `run`/`call` with
`--mode <pure|project|strict>` (default: `project`).

| Mode | Intended guarantee | Actual status in this build |
|---|---|---|
| `pure` | No external I/O; memory computation and budgets only. | **Enforced.** `Capabilities::pure()` = `console`, `math` only; every FS/foreign/plugin grant flag makes the CLI exit 2 with "pure mode prohibits external I/O grants". |
| `project` | Operator-approved project resources under `CapabilityHost` + bounded resources. | **Enforced for the implemented operations.** Defaults equal `pure`; `--allow-read`, `--allow-write`, `--allow-foreign`, `--allow-plugin` add grants. `resource.open` supports stdout write, granted project-file read/write (ADR-0025/0026) and granted in-process provider kinds (ADR-0033); network/process remain unsupported. Sandboxing is cooperative (in-process checks), not OS isolation; in-process providers are trusted, not sandboxed. |
| `strict` | Untrusted documents under real OS isolation. | **Linux: enforced when the runtime probe passes (ADR-0021).** `resource.open` is **refused** in strict except for the existing `stdio/stdout` console channel (console grant); file/network/process resources remain refused until a provider passes the isolation lane (CM-A11, ADR-0030). `PlatformCapabilities::detect()` runs a real Bubblewrap smoke test; success reports `Available { mechanism }`, failure reports `Unavailable { reason }`. Strict L1 supports the interpreter, console, native Math and the read-only bundled stdlib (published at `/run/marqdo-stdlib` for the Compat-C print chain); `--allow-read/--allow-write/--allow-foreign/--allow-plugin/--plugin/--trust-native-plugin/--kernel/--trace-file` are rejected with exit 2. macOS/Windows and Linux hosts without the mechanisms remain `Unavailable`; there is **no fallback** to an unrestricted process. |

`marqdo inspect <file> --platform` prints the detection report, for example:

```json
{
  "os": "linux",
  "arch": "x86_64",
  "strict": {"state": "available",
             "mechanism": "bubblewrap 0.11.1 (user/mount/PID/net/UTS/IPC namespaces + no-new-privs + rlimits)"},
  "worker_processes": true
}
```

On a host that cannot create the namespaces the same block is
`{"state": "unavailable", "reason": "..."}` and strict refuses. Design rule
(blueprint §9): a cooperative in-process check is not a sandbox. See ADR-0021
for the Linux construction, L1 scope and non-goals; ADR-0019 still governs
every other platform.

## 2. Capability grants

`Capabilities` (`crates/marqdo-host/src/capability.rs`) is the effective grant
set for one run:

| Field | Default (`pure`/`project`) | Used by |
|---|---|---|
| `console: bool` | `true` | `resource.open("stdio","stdout",["write"])` and the stdlib print chain; a missing grant is `permission.denied` and output is also bounded by the output budget |
| `math: bool` | `true` | `math.*` host ops |
| `fs_read: Vec<String>` | empty | `fs.read` and project `resource.open("file", path, ["read"])` |
| `fs_write: Vec<String>` | empty | `fs.write` and project `resource.open("file", path, ["write"])` |
| `net_hosts: Vec<String>` | empty | `Capabilities::check_net` exists, but no host op or builtin performs network access in this build |
| `process: bool` | `false` | required together with `foreign` for `foreign.run`/`foreign.call` |
| `foreign: bool` | `false` | foreign kernels |
| `plugins: Vec<String>` | empty | `plugin.invoke` and `StdHost::load_plugin` |
| `resource_providers: Vec<String>` | empty | registered in-process `ResourceProvider` kinds; project mode only (ADR-0033) |

Requests versus grants:

* `marqdo.yaml` `capabilities:` is a **request**. It is parsed and validated
  (`crates/marqdo-package/src/lib.rs`) but is not consulted when building the
  host capability set: only CLI flags/grant APIs produce grants. Declaring a
  capability never grants it.
* `marqdo.yaml` `plugins:` likewise declares requirements only; it does not
  install, load or authorize a plugin.
* CLI grant flags (`crates/cli/src/main.rs::SandboxArgs`):
  * `--allow-read <GLOB>` and `--allow-write <GLOB>` (repeatable, project mode
    only);
  * `--allow-foreign` (project mode only; sets `foreign = true` and
    `process = true`);
  * `--allow-plugin <PACKAGE-NAME>` (repeatable, project mode only; matches the
    manifest `plugin.name`, not the call namespace);
  * `--plugin <PATH>` (repeatable, loads a plugin after the trust check);
  * `--trust-native-plugin` (explicit in-process native trust decision).
* There is no network grant flag and no plugin-worker flag in this build.

### 2.1 Filesystem grants are project-relative globs

`fs.read`/`fs.write` paths go through `ProjectFs::resolve` **before** the
capability check:

* empty paths, absolute paths, `..` components and root/prefix components are
  rejected with `permission.denied`;
* `.` components are dropped; the normalized relative string uses `/`;
* the capability patterns are matched with the `glob` crate against that
  normalized relative path; a leading `./` is stripped on both sides, so
  `--allow-read ./data/**` matches `data/a.csv` and does not match
  `secrets/key`; `--allow-write output/**` does not grant reads;
* an invalid glob pattern is treated as non-matching (`Pattern::new(...).
  unwrap_or(false)`), so it silently grants nothing (there is no diagnostic).

Additional FS limits (`crates/marqdo-host/src/fs.rs`):

* reads are capped at `MAX_READ_BYTES = 8 MiB`, must be regular files, must be
  UTF-8, and the canonicalized path must stay inside the project root;
* writes create parent directories; before writing, the **closest existing
  ancestor** is canonicalized and must stay inside the project root.

Known caveat: for writes, only the closest existing ancestor is checked and the
final write uses the non-canonical path. A symlink created between the check
and the write (TOCTOU) could escape the root. Reads canonicalize the final path
before reading. Full symlink/race hardening is an open P4 item.

### 2.2 Checks happen at execution points, not only at parse time

`Host::invoke` is the only path to effects. `StdHost::dispatch` routes every
`HostOp` and the checks run immediately before the effect:

* `fs.read` → `capabilities.check_read(path)` then `ProjectFs::read`;
* `fs.write` → `check_write` then `ProjectFs::write`;
* math ops → `capabilities.math` (in `crates/marqdo-host/src/kernels.rs`);
* foreign ops → `capabilities.check_foreign` (requires `foreign && process`);
* plugin invoke → the **complete export name** must be registered, then
  `capabilities.check_plugin(package)` on **every call**; loading also checks
  the same grant once;
* plugin host-capability requests → `CapabilityBroker` checks the package and
  effective grants and **executes** granted `fs.read` through `ProjectFs`,
  returning the real file bytes. Other core capabilities (`fs.write`,
  `net.request`, `process`, console/math/log/...), and vendor capabilities
  without a trusted executing handler, return `capability.unavailable`; they
  never pretend an empty reply is success. Unknown/vendor names without a
  trusted policy entry become `capability.unavailable`/`permission.denied`;
* print → output budget only.

Static analysis may warn, but it never replaces these runtime checks. There is
no host bypass for tests: tests use the same `StdHost`/`Capabilities` types.

## 3. Resource budgets

`Budget` (`crates/marqdo-host/src/budget.rs`) is per run:

| Limit | Default | Enforcement |
|---|---|---|
| evaluation fuel | `DEFAULT_FUEL = 5_000_000` | 1 unit per statement, 1 per expression node, 1 per branch condition, 1 per loop iteration; exhaustion → `resource.exhausted` |
| call stack depth | `DEFAULT_STACK_LIMIT = 256` | checked before every function call → `resource.exhausted` |
| program output bytes | `DEFAULT_OUTPUT_BYTES = 1 MiB` | `print` output and `fs.read` contents are charged; overflow → `resource.exhausted` |
| wall clock | none by default | `--timeout <seconds>` sets a deadline; checked between statements/iterations and before loop conditions → `resource.exhausted`. It does **not** preempt a running host call. |
| addressable memory | 64 MiB committed total, 16 MiB per region, 64 KiB pages; `--max-memory` / `--max-region-memory` override | Runtime-owned `MemoryStore` (ADR-0022); create/grow checks both budgets before allocation and failed growth is atomic → `resource.exhausted`. Memory is pure computation and needs no capability grant in any mode. |
| resource streams | stdout writes charged to the existing output budget; file reads and writer buffers bounded by the 8 MiB per-file limit | Host-owned resource table (ADR-0025/0026); output budget is checked before a stdout write is buffered; a failed check produces no bytes; writer buffers flush only on `close`. No separate I/O byte budget yet. |

CLI overrides (`run`/`call`): `--fuel`, `--stack`, `--max-output`,
`--timeout`. Additional fixed limits:

* foreign kernel: `StdHost.foreign_timeout = 10s` and
  `foreign_output_limit = 1 MiB` (stdout+stderr combined) per call.
  These are not configurable from the CLI in this build;
* plugin wire: request ≤ 16 MiB, result ≤ 64 MiB, error ≤ 1 MiB, metadata
  ≤ 64 KiB (see `include/marqdo_plugin_abi_v1.h`);
* math kernel: 1 MB source, 256 parse depth, 512 AST nodes, exponent ≤ 10,000,
  exact-result ≤ 1,000,000 bits (`crates/marqdo-math`).

These are cooperative budgets in the interpreter and the supervisor. Only
foreign child processes are terminated by the host (kill on timeout/output
overflow). In-process math and native plugins are not OS-limited.

## 4. Effect tracing

`EffectTrace` records one entry per `Host::invoke` when enabled
(`--trace`, or implicitly `--json` for `run`/`call`). Entry fields:

```json
{"op":"print","module":"hello.mq.md","span":{"start":43,"end":57},
 "duration_micros":3,"result":"ok"}
```

* `op` is the stable `HostOp::name()` (`print`, `math.eval`, `foreign.call`,
  `fs.write`, `plugin.invoke`, ...);
* `module` is the initiating module id; `span` is the call site when known;
* `result` is `"ok"` or the failure code (for example
  `permission.denied`);
* argument values and file contents are **not** recorded, so ordinary secrets
  do not leak into traces by design. Paths are also not recorded today.
* In text mode trace entries are printed to stderr (`trace: ...`); in JSON
  mode they are a `trace` array next to the program stdout string. Program
  output is always separate from diagnostics/trace (stdout vs stderr).

## 5. Foreign kernels: process isolation, not a sandbox

`crates/marqdo-foreign/src/lib.rs` runs Python/Node one-shot:

* the program is written to a private temp file (mode 0600 on Unix), the child
  runs with a fresh private temp directory (mode 0700) as cwd, and both are
  removed on drop;
* direct `argv` exec (no shell, no interpolation); stdin carries the versioned
  request envelope; stdout/stderr are always piped and captured;
* the environment is cleared except `PATH`;
* on Unix the child is a process-group leader (`setpgid`) and the wall-clock
  deadline/output overflow kills the whole group, including grandchildren; a
  2-second reader grace prevents a surviving pipe holder from hanging the
  host. Non-Unix targets only kill the direct child;
* a non-zero exit is data (`RunStatus::Failed`, stderr preserved); a timeout is
  `TimedOut` for `run` and an error for the typed `call`.

What this is **not**: an OS sandbox. The child runs with the host user's
privileges, can open absolute paths, use the network and spawn further
processes. Clearing the environment and changing cwd are hygiene, not
confinement. Grandchildren are not killed when the direct child is killed.
Strict L1 rejects Foreign execution before any sandbox starts, so foreign
execution is only reachable in `project` mode with `--allow-foreign`. L2
inheritance is future work (ADR-0021).

## 6. Plugins: in-process native trust boundary

* The only plugin transport implemented in this build is **in-process native
  loading** (`libloading`) after `manifest.validate()`, exact target-triple
  artifact resolution, sha256 verification, ABI negotiation and export
  metadata cross-checking (`crates/marqdo-plugin-abi/src/loader.rs`).
* Loading requires **three** independent things:
  1. `--plugin <manifest-or-dir>` names the plugin;
  2. `--trust-native-plugin` is an explicit unsafe trust decision;
  3. the manifest package name is granted with `--allow-plugin <name>` (checked
     again on every invocation).
  Without trust, `StdHost::load_plugin` returns `permission.denied` with the
  hint that no isolated worker transport exists. There is no isolated worker,
  Wasm component or out-of-process plugin path in this build.
* A native plugin runs in the host address space. It can read host memory,
  call arbitrary syscalls, bypass the capability model entirely and crash or
  corrupt the host process. **A C ABI is not a sandbox.** `--trust-native-plugin`
  must be treated as equivalent to running the plugin's code as the host user.
* Every export is registered by its complete dotted name; a plugin may export
  several prefixes, and a conflicting name rejects the whole load atomically.
  `check --plugin <manifest>` (or `run/call --plugin`) validates export
  signatures statically without loading a shared library.
* Plugin call sites are written as `<export>(...)` in a function
  body (for example `solver.add(a=20, b=22)`). The namespace is derived from
  the **first** manifest export prefix. Calls are made through
  `HostOp::PluginInvoke` with a typed `wire_version` 1 request; the plugin's
  typed response becomes a Marqdo value (`Unit` when absent). Named arguments
  are required in this build; positional plugin arguments are `runtime.type`.
* Plugin errors map to structured runtime errors: ABI → `plugin.abi_mismatch`,
  capability → `permission.denied`, wire/value → `runtime.type`, unsupported →
  `kernel.unavailable`, everything else → `plugin.failure`. An unloaded
  namespace is `symbol.unknown`.
* The plugin's host-capability requests are denied by default: the CLI loads
  plugins with `LoadOptions::default()`, i.e. **no capability handler**, so
  every `request_capability` call fails. The `requires:` list in `plugin.yaml`
  is only a declaration.
* `marqdo plugin inspect` never loads code: it validates the manifest and
  verifies the artifact hash only.
* Installation, inspection, loading and authorization are four separate
  decisions. Copying a plugin into the project or declaring it in
  `marqdo.yaml` grants nothing.

Known limitations: single namespace per plugin (derived from the first export),
no hot unload/reload, no plugin capability bridge from this CLI, native crash
is a host crash, `plugin.invoke` cannot be used in `pure`/`strict` mode.

## 7. `check` and `inspect` are side-effect free

Neither command executes project code. Specifically:

* `check` parses, indexes, maps to IR, runs static call validation and reports
  diagnostics. It never calls a function, never spawns a foreign kernel, never
  loads a native plugin and never writes files.
* `inspect` with `--ast`, `--units`, `--signature`, `--ir`, `--refs` or
  `--platform` does the same static work and prints metadata. `--refs` may
  **read** other project source files on demand to resolve link targets, but
  only parses/analyzes them; nothing is executed or written.
* `plugin inspect` reads `plugin.yaml`, validates it, resolves the artifact for
  the host target and computes/compares its sha256. It does not `dlopen`,
  instantiate or call the plugin (`"loaded": false` in JSON output).
* `run`/`call` first perform the same static load; if any diagnostic has error
  severity, execution is refused before the host or any kernel is touched.
* `fmt` only normalizes line endings/trailing newlines and writes the document
  when not in `--check` mode; it does not execute anything.

## 8. Open security items

1. Strict L1 is implemented and VM-verified on Linux (ADR-0021); strict L2
   (Worker/Foreign inheritance) is not implemented, so those surfaces are
   refused in strict.
2. `console` capability is not enforced; print is always allowed.
3. Network capability exists in the model but no host operation uses it; there
   is no `--allow-net` flag.
4. Write-path symlink TOCTOU (see §2.1).
5. Plugin **worker crash isolation** and the worker capability bridge are
   implemented (not usable in strict L1): both native and worker plugins can
   reach the broker's
   `fs.read` execution when granted. Other core capabilities and vendor
   handlers remain unavailable. Broker execution is a cooperative API
   contract, not an OS sandbox; a worker and a native plugin can still issue
   direct syscalls.
6. `plugin_handle` wire values are structurally reconstructable, but host
   authority comes only from the `PluginHandleTable` owner/run/generation
   check; untrusted plugin/foreign output containing a handle is rejected
   (ADR-0014, `value-wire.md` §4.1).
7. Budgets do not cover all allocation paths (for example list/table growth
   inside the interpreter is bounded only by fuel and memory).
8. `--json` run output includes the full program stdout string; that is the
   program's own output, not a host secret, but it is embedded in the JSON
   document consumers must parse.
