# Marqdo 3.0.0-rc.3 — known limitations

This file ships in the release archive as `KNOWN_LIMITATIONS.md`. It is part of
the release record: it defines the platform and feature scope for the release candidate.
Items marked **unsupported** must fail closed, not be silently emulated.

## Platform support

| Platform | RC scope | Notes |
|---|---|---|
| Linux x86_64 (glibc 2.39 baseline) | **Supported release platform** | The archived `marqdo` and `marqdo-provider-worker` binaries target `x86_64-unknown-linux-gnu` and are the only binaries this release claims. |
| Linux aarch64 | Not built | No release artifact; source may build. |
| macOS (arm64/x86_64) | Experimental, not claimed | Core CLI may work, but no macOS provider worker, no native plugin ABI claim and no macOS release artifact. |
| Windows x86_64 | Experimental, not claimed | Core CLI may compile/run; the plugin/provider C ABI and provider worker are not claimed. |

`pure` and `project` are **language execution modes**, not OS sandboxes.
`project` may access resources only through explicit grants, but it runs as the
user and is not a security boundary against hostile code.

## `strict` isolation

`strict` is Linux-only and probe-gated. The RC release requires same-SHA
G3 evidence from a non-root disposable VM. The CLI fails closed when the
runtime probe is unavailable.

Even when available:

* strict L1 covers the interpreter, console and read-only bundled stdlib;
* strict L2 (Worker/Foreign/Provider inside strict) is **unsupported** and every
  such flag is refused with exit 2;
* rlimits are used for resource limits, not cgroup-v2 accounting;
* the network is denied with a network namespace, not a seccomp allowlist;
* a PASS is evidence for isolation correctness, never for performance.

## Extensions

* **Callable plugin ABI v1**: native and worker transports exist. Native loads
  into the host process and requires the explicit unsafe
  `--trust-native-plugin`; a C ABI is not a sandbox. Worker is crash isolation
  with handshake/call timeouts, not an OS sandbox. Wasm, hot unload and a full
  worker capability broker are unsupported. Windows C ABI is not claimed.
* **Resource Provider v2**: Linux x86_64 worker transport only. The
  `marqdo-provider-worker` helper must be next to the `marqdo` executable;
  missing/renamed helpers fail closed (`ext.worker_unavailable`) and PATH is
  never consulted. A provider is not a sandbox.
* **Foreign kernels**: any language can be registered with `kernel.json`, but
  the kernel process is a normal child process; it is not sandboxed, and the
  host must refuse unauthorized, digest-mismatched, timed-out or
  wire-incompatible kernels.
* **CLI tools**: a tool is an ordinary user-authorized executable and does
  **not** inherit `pure`/`project`/`strict` guarantees from the documents it
  reads. `MARQDO_BIN`, `MARQDO_TOOL_PROTOCOL`, `MARQDO_PROTOCOL_ANALYSIS` and
  `MARQDO_PROTOCOL_TRACE` are the only helpers the CLI injects.
* **Extension selection** (`marqdo.yaml.extensions`, lock v4) is never
  authorization. `--allow-plugin`, `--allow-provider` and `--allow-foreign`
  are still required.

## Compatibility and versioning

* `3.0.0-rc.3` is a **prerelease**. The changelog lists changes from the beta.
* The product version is independent of protocol versions: Analysis,
  Observation, CLI Tool, Plugin ABI v1, Foreign wire v1, Provider manifest v2
  and lock v4 keep their own identifiers.
* The official bundled stdlib keeps its independent package version `0.1.0`.
  A plugin `.mq.md` package version such as `0.1.0` is also independent.
* `project.marqdo` in `marqdo.yaml` is enforced in this release. SemVer
  prerelease rules apply: `>=3.0.0,<4.0.0` does **not** accept a prerelease core. Packages and Tools can use a prerelease range such as
  `>=3.0.0-beta.1,<4.0.0`.
* Old `cflmy/marqdo` source syntax, package format and plugin ABI v2 are not
  compatible. There is no `--legacy` or downgrade path; migration is a separate
  documented workflow.

## Artifact integrity

Archives are accompanied by SHA256 checksums and `RELEASE_INFO.json`
(`source_sha`, target triple, toolchain, binary hashes). Checksums prove content
integrity only; they are **not** a publisher signature. A digest obtained from
an untrusted Git URL does not make the code trustworthy.

## Execution Events v1

* Implemented: live stdout/stderr chunks, lifecycle/terminal events, call
  instance ids, branch/loop/effect events, explicit bounded value observation.
* Not implemented: memory region/budget metadata (OBS-06/P2), raw memory
  content, breakpoints/stepping/time travel, and a bidirectional debug
  protocol. Do not infer a debugger from this protocol.
* `--mode strict` currently rejects `--output jsonl` explicitly; there is no
  silent fallback to `project`.
* The live lane is tested on Linux x86_64; macOS/Windows are unverified for
  this option for this candidate.
* In-process native plugins are rejected in event mode
  (`ext.event_stream_unsupported_native_plugin`) because they share the process
  and can write the protocol fd directly; worker plugins remain usable.
* Observation lowers confidentiality when values are explicitly enabled. Text
  needs a second opt-in; View/agents must not enable these flags by default.

## Deliberately deferred

strict L2, Wasm plugins, macOS/Windows provider worker, automatic uv/npm
environment creation, a distributed package registry, JIT execution, built-in
Knowledge/Proof/View commands, a mature async/concurrency model, and platform
installers. The planned direction is external Tool repositories consuming the
public protocols, not new core commands.

## Beta migration

The former core `math.*` calls fail static validation with `symbol.unknown`.
Use a pure `.mq.md` library or an explicitly selected ABI backend. See
[the math library migration](libraries.md).
Task/Channel and Resource Control are outside this RC's protocol scope.
