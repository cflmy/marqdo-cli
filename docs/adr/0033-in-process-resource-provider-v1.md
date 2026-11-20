# ADR-0033: In-process ResourceProvider v1 (CM-A09 first slice)

Status: **Accepted (in-process slice only)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A11, P3/P5), ADR-0025, ADR-0026, ADR-0031
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-provider-abi-cm-a09.md`

## Context

ADR-0025/0026 hard-code two resource kinds (`stdio/stdout`, project `file`) in
the host. CM-A09 requires a versioned, discoverable provider contract so a new
device/OS endpoint can be added **without** changing the expression parser,
`Value` model, runtime dispatch or `HostOp` enum. The RFC warns against a
second shadow plugin ecosystem: providers should reuse the same host/ABI
infrastructure where possible.

The full C ABI/Worker resource protocol is a larger artifact (C structs, wire
buffers, ownership, process isolation). This ADR freezes the **in-process
trusted provider** slice first; a Worker/C ABI binding can implement the same
contract later. In-process providers are explicitly **not** a sandbox.

## Decision

### 1. Provider contract v1

```rust
pub trait ResourceProvider: Send {
    fn name(&self) -> &str;                       // stable grant name
    fn protocol_version(&self) -> u32 { 1 }       // must be 1 in this slice
    fn kinds(&self) -> &[&str];                   // resource kinds it serves
    fn open(&mut self, kind: &str, descriptor: &str, rights: &[String],
            span: Option<Span>) -> Result<u64, RuntimeError>; // provider token
    fn read(&mut self, token: u64, max_bytes: usize,
            span: Option<Span>) -> Result<ResourceRead, RuntimeError>;
    fn write(&mut self, token: u64, bytes: &[u8],
             span: Option<Span>) -> Result<usize, RuntimeError>;
    fn close(&mut self, token: u64, span: Option<Span>) -> Result<(), RuntimeError>;
}
```

* `ResourceRead { bytes, eof }` is the same host type used by built-in streams.
* The **host** still issues the opaque language `ResourceHandle`; the provider
  token is private to the provider and never enters a `Value`, wire message or
  trace.
* Bytes cross the provider boundary as copied slices for the duration of a
  call; no provider may retain a `Memory` pointer across calls or after a
  `grow`.
* `open` returns a provider token; `read/write/close` must reject unknown,
  stale or closed tokens with a structured error. The host checks the language
  handle before dispatching.

### 2. Registration

`StdHost::register_resource_provider(Box<dyn ResourceProvider>)`:

* rejects `protocol_version != 1`;
* rejects an empty `kinds()` list;
* rejects a provider name or kind that is empty, `stdio`, `file`, or already
  registered (no silent shadowing);
* stores the provider under each declared kind and its stable name for grants.

Registration is a host/embedding API, not a language import. Importing a
`.mq.md` package never registers or authorizes a provider. Installation and
authorization remain separate (INV-06).

### 3. Authorization and modes

* `Capabilities` gains `resource_providers: Vec<String>` and
  `allow_resource_provider(name)`; a matching name is required at
  `resource.open` time.
* `project` only. `pure` and `strict` refuse provider kinds even when
  registered; strict remains fail-closed (CM-A11). Existing stdio/file rules
  are unchanged.
* A provider's own rights/descriptor validation is not a substitute for the
  grant; denial is `permission.denied` with a diagnostic.
* In-process provider code shares the host address space and is **not**
  sandboxed. Only explicitly trusted providers may be registered; the API
  documentation and ADR state this.

### 4. Runtime surface

No new `Value`, `HostOp`, Markdown node or grammar. A language program still
writes:

```text
resource.open("<kind>", "<descriptor>", ["read"])
```

The existing `resource.read/write/close` and `Result` shapes are reused. The
provider name is used only for the grant check and trace metadata; the language
does not need to know it beyond the grant.

### 5. What remains for CM-A09

* C ABI / Worker resource transport (buffer handoff, ownership, crash isolation,
  version negotiation through the existing plugin ABI).
* Provider discovery/installation from packages, signatures and platform
  support matrices.
* Network/socket/device providers themselves.
* Strict-mode provider verification.

This ADR is a contract for trusted in-process providers, not a second plugin
ecosystem.

## Consequences

* A new endpoint can be implemented as a `ResourceProvider` and registered
  without touching parser/runtime/`HostOp`; language code uses the same
  `resource.*` calls.
* The provider token cannot be forged into a language handle; only the host
  creates handles.
* In-process providers can crash/block the host; they are trusted. Worker C ABI
  is still needed for untrusted extensions.
* `Capabilities` grows one grant list; CLI exposure is deferred because there
  is no language-level provider installer yet.

## Verification

`docs/evidence/2026-11-20-provider-abi-cm-a09.md` records a mock read/write
provider registered without parser/runtime changes, grant/pure/strict denial,
protocol/kind conflict registration errors and the full workspace gate.
