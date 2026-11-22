# ADR-0062: Resource-operation ownership and core resource kinds

Status: Accepted (post-`3.0.0-beta.1`, RFC-EXT-0003 H-03 / H-04).
Supersedes nothing; refines ADR-0025, ADR-0033, ADR-0041–0050 and
[`spec/resource-provider-v2.md`](../../spec/resource-provider-v2.md).

## Context

`Host::resource_request` had grown a mixed catalogue: operating-system
primitives (`process.run`, `fs.*`, `env.get`, `clock.now`, `random.bytes`) next to
convenience services implemented inside the host (`regex.*`, `compress.*`).
Nothing recorded which entries were intentional core surface and which were domain
convenience that should become external extensions. Separately, six resource kinds
were reserved by `spec/resource-provider-v2.md`, but the trusted host registration
path only rejected `stdio` and `file`; only the CLI manifest validator had the full
list. A host caller (`register_resource_provider`, `register_c_resource_provider`,
`register_worker_resource_provider`, `register_provider_package`) could therefore
register a provider named `tcp` or declaring `tls` and shadow core dispatch.

## Decision

### 1. One core resource-kind registry

`marqdo_host::CORE_RESOURCE_KINDS` is the single source of truth:
`stdio`, `file`, `tcp`, `tls`, `udp`, `tcp-listener`. Both the CLI provider
manifest validator and every host registration path use it. A provider whose `name`
is a core kind, or which declares a core kind, is rejected before it enters the
resource table with `ResourceUnsupported` (CLI: `ext.manifest_invalid`). A change
to the list requires this ADR and the spec to change with it.

### 2. Two operation classes

`resource_request` is closed over the following reviewed operations:

| Class | Operations | Owner |
|---|---|---|
| Core OS primitive | `process.run`, `env.get`, `clock.now`, `random.bytes`, `fs.read_at`, `fs.append`, `fs.walk`, `fs.exists`, `fs.metadata`, `fs.list`, `fs.mkdir`, `fs.remove`, `fs.rename` | host / OS |
| Host-held built-in service | `regex.match`, `regex.captures`, `compress.gzip`, `compress.gunzip` | host library, existing grants |

`regex.*` and `compress.*` are existing host-held services with their own
ADRs; they are not a precedent for adding more domain functions to the host.
Project mode and the corresponding capability grant are required for every entry.

### 3. Extension boundary rule

New business or domain operations **must not** be added to `resource_request`.
A domain function belongs in a pure `.mq.md` package, a Callable Plugin ABI
backend, or a Resource Provider — selected and authorized through the normal
extension path. Adding an operation to the core table requires a new accepted ADR and
an update to [`docs/architecture/resource-operations.md`](../architecture/resource-operations.md),
`spec/resource-provider-v2.md` and the architecture gate
`crates/cli/tests/architecture_boundaries.rs`.

## Consequences

- A host-side provider can no longer shadow a core kind even when it did not pass
  through the CLI validator; the CLI and host fail with consistent wording.
- The reviewed operation list is executable evidence: the architecture test extracts
  the literals from `resource_request` and fails on unrecorded additions.
- `regex.*` / `compress.*` keep working unchanged; moving them out is a separate
  ADR and is not implied by this one.
- No CLI flag, language keyword, wire version or Analysis/Trace schema changes.
