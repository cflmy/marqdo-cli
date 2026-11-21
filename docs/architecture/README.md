# Architecture RFCs: current state, decisions and design input

Status: **index**. The single current architecture overview is
[`CURRENT.md`](CURRENT.md). This page only indexes frozen decisions, design
input and evidence. It is not a second running-status page.

- Current architecture and four state dimensions: [`CURRENT.md`](CURRENT.md).
- Executed status and open blockers: [`../STATUS.md`](../STATUS.md).
- Frozen behavior: [`../../spec/`](../../spec/); release gates: [`../security-gate.md`](../security-gate.md).
- Historical planning snapshots: [`../history/README.md`](../history/README.md).

## Design input RFCs (Proposed, not implemented behavior)

| Document | Status | What it is |
|---|---|---|
| [Architecture audit and refactor RFC](Marqdo_CLI_Latest_Architecture_Audit_and_Refactor_RFC.md) | **Review input** | Responsibility convergence, stdio/file/stream correctness, CLI/Host split and the current migration order. It is not evidence that a phase is done. |
| [Compute machine RFC-0001](../MARQDO_COMPUTE_MACHINE_RFC_0001.md) | **Proposed** | Opaque `Memory`, addressable regions, wire/host rejection. |
| [Core and stdlib migration RFC-0002](../MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md) | **Proposed** | Minimum core, `.mq.md` stdlib and system-capability migration. |

The implemented first slices of these RFCs are recorded in `CURRENT.md` and
`../STATUS.md`; the RFC text itself never proves that code exists.

## Decisions already frozen

| RFC item | ADR | State |
|---|---|---|
| CM-A01 Memory identity/lifetime | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A02 page size, growth, budgets | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A03 place assignment and evaluation order | [0022](../adr/0022-addressable-memory-and-place-assignment.md) | Accepted |
| CM-A04 minimal structure observation/construction | [0023](../adr/0023-minimal-structure-observation-and-construction.md) | Accepted |
| CM-A05 recoverable `Result` convention | [0024](../adr/0024-recoverable-result-convention.md) | Accepted |
| CM-A06 minimal Resource/Stream ABI | [0025](../adr/0025-minimal-resource-stream-abi.md) | Accepted (first slice) |
| CM-A06 buffered project-file write | [0026](../adr/0026-buffered-project-file-write-streams.md) | Accepted |
| CM-A07 print migration staging / Compat-A | [0027](../adr/0027-print-migration-compat-a.md) | Accepted (Compat-A only) |
| CM-A10 bundled stdlib / `std:` imports / digest | [0028](../adr/0028-bundled-stdlib-and-std-imports.md) | Accepted (first slice) |
| Full-value stdlib formatting / FunctionRef+PluginHandle observation | [0029](../adr/0029-full-value-formatting-and-handle-observation.md) | Accepted |
| CM-A07 Compat-B print routing | [0030](../adr/0030-print-compat-b-routing.md) | Accepted |
| CM-A07 Compat-C remove legacy print formatting | [0031](../adr/0031-print-compat-c-remove-host-formatting.md) | Accepted |
| CM-A08 table logical value / stdlib algorithms / T0 storage | [0032](../adr/0032-table-logical-value-and-stdlib-algorithms.md) | Accepted |
| CM-A09 in-process ResourceProvider v1 | [0033](../adr/0033-in-process-resource-provider-v1.md) | Accepted (trusted in-process slice) |
| P5 blocking TCP resource endpoint v1 | [0034](../adr/0034-tcp-resource-endpoint.md) | Accepted (IP-literal, no DNS/TLS) |
| CM-A09 worker resource-provider transport v1 | [0035](../adr/0035-worker-resource-provider-transport.md) | Accepted (trusted helper) |
| CM-A09 C ABI ResourceProvider v1 | [0036](../adr/0036-c-abi-resource-provider.md) | Accepted (trusted native adapter) |
| CM-A09 provider package manifest v1 | [0037](../adr/0037-provider-package-manifest.md) | Accepted (host-side install) |
| CM-A09 C ABI provider worker binding v1 | [0038](../adr/0038-c-abi-provider-worker-binding.md) | Accepted (trusted helper) |
| Stdlib extraction deferral | [0039](../adr/0039-stdlib-extraction-intent.md) | Accepted (in-repo for now) |
| Stdio stdin/stderr plus stdlib stream/console APIs | [0040](../adr/0040-stdio-stdin-stderr-and-stdlib-streams.md) | Accepted (stdin read, separate stdout/stderr, product stdlib) |
| Non-streaming `resource.request` `fs.*` operations | [0041](../adr/0041-resource-operation-fs.md) | Accepted (project mode, grant-checked, recoverable fs Result) |
| `clock.now` resource operation | [0042](../adr/0042-clock-resource-operation.md) | Accepted (project mode, Unix millis) |
| `env.get` resource operation | [0043](../adr/0043-env-resource-operation.md) | Accepted (exact-name project grant) |
| `process.run` resource operation | [0044](../adr/0044-process-resource-operation.md) | Accepted (exact program grant, project only, not a sandbox) |
| `fs.append`/`fs.read_at`/`fs.walk` | [0045](../adr/0045-fs-append-read-at-walk.md) | Accepted (bounded content, directory walk) |
| `random.bytes` resource operation | [0046](../adr/0046-random-resource-operation.md) | Accepted (explicit `--allow-random`, project only) |
| Table physical storage (T1/T2/T3), strict stdin/stderr/file/network resources | — | **Not decided**; do not implement by implication |

## Evidence and status

- Executed test counts, platform lanes and release blockers live in
  [`../STATUS.md`](../STATUS.md). Do not repeat them here.
- SHA-pinned fact snapshots live in [`../evidence/`](../evidence/).
- Superseded planning input lives in [`../history/`](../history/), and its
  remaining files carry a **Historical** banner.

## What a future phase must not do

* add a top-level CLI command or Markdown keyword for memory, allocation or I/O;
* add a `HostOp` for every new stdlib function;
* turn managed `List`/`Map`/`Table` into aliased mutable storage;
* expose a host pointer, let a handle cross the wire, or claim block-level
  safety for user allocators;
* make `strict` accept an unverified provider by default;
* claim an RFC phase is complete without the closure and negative evidence its
  phase gate requires;
* edit a gate result in `../STATUS.md` without an executed log or a source path.
