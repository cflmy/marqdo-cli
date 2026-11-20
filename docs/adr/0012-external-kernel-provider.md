# ADR-0012: External foreign-kernel providers

Status: **Accepted**
Date: 2026-11-20

## Context

`marqdo-foreign` hard-codes `SUPPORTED_LANGUAGES = ["python", "javascript"]`
and `normalize_language`/`program_candidates` branch on those names. Adding R,
Julia or SQL requires changing the crate and shipping a new interpreter even
though foreign kernels are optional, externally installed programs.

The M0.6 plan (E05, R2) requires a provider registry. A provider is an
**execution kernel** for an inert `Code` value, not a CLI Tool and not a C ABI
plugin.

## Decision

### 1. Descriptor and protocol

```rust
pub struct KernelDescriptor {
    pub provider_id: String,       // `python`, `javascript`, `mocklang`
    pub languages: Vec<String>,    // accepted code-fence labels/aliases
    pub wire_version: u32,         // 1 for the current typed JSON envelope
    pub entrypoint: PathBuf,       // trusted provider executable/script path
    pub digest: Option<String>,    // content digest of the provider program
    pub permissions_requested: Vec<String>,
}
```

Providers implement the existing `foreign` wire protocol: one request envelope
on stdin, stdout must be exactly one typed wire value for `call`, stderr and a
non-zero exit are errors. A provider receives no secret environment beyond
`PATH` unless explicitly granted.

### 2. Registry

`marqdo_foreign::KernelRegistry` is constructed by the host/CLI:

* built-in providers are registered for `python`/`python3`/`py` and
  `javascript`/`js`/`node`/`nodejs` with their existing one-shot command
  lines;
* additional providers come from an explicit, versioned `kernel.json`
  descriptor (or a future `marqdo.yaml` kernel section);
* registration is static, explicit and user-controlled; no PATH scan for
  arbitrary providers and no automatic download.

Lookup happens **only** when `foreign.run`/`foreign.call` is explicitly
invoked. Parsing/rendering a `python`/`r`/unknown fence always yields an inert
`Code` value; an unregistered label is legal until execution, then
`kernel.unavailable`.

### 3. Isolation and authorization

A provider process is a foreign kernel, never the Marqdo interpreter. Spawning
requires `allow_foreign` (or an explicit kernel grant). The host applies
timeout, output cap, process-tree cleanup and environment clearing as today.
A provider descriptor's `permissions_requested` is advisory and cannot grant
filesystem/network access; any such access must be implemented by the provider
inside the same OS-level limits as other foreign processes.

Tool Protocol is deliberately separate: a CLI Tool is a user-invoked native
program; a Kernel Provider executes code requested by an `.mq.md` program. They
do not share registries or trust decisions.

### 4. Versioning

`wire_version` must equal the host's supported version (1) or execution fails
with `kernel.wire_version`. The language labels are case-insensitive and
trimmed; duplicate labels across providers are a registration conflict. The
existing Python/Node detection remains as the built-in provider implementation,
not a special case in the call path.

## Alternatives

1. Keep the hard-coded language list. Rejected by E05.
2. Let a provider be any executable found on PATH by language name. Rejected:
   PATH hijacking and implicit execution.
3. Reuse the C ABI plugin manifest. Rejected: different trust model, lifecycle
   and wire protocol.
4. Execute unknown languages via a generic shell. Rejected: no explicit
   provider, no deterministic protocol, no capability boundary.

## Compatibility

`foreign.run/call` keep their names and wire protocol. Existing Python/Node
programs behave the same. Unknown labels remain inert values; only an explicit
call errors. A future registry source can add providers without changing the
interpreter.

## Security

A provider is untrusted code by default. The host does not treat a descriptor as
a sandbox; it applies the same child-process limits and explicit grant. A
descriptor cannot grant itself core capabilities, and a tool cannot become a
kernel provider implicitly.

## Testing

* register `mocklang` without changing `marqdo-foreign` and execute a typed
  call, with timeout/output-limit/denial cases;
* unregistered label yields `kernel.unavailable` only on call;
* duplicate language labels and wire-version mismatch are rejected;
* built-in Python/Node tests continue to pass when the interpreters exist.

## Migration & rollout

R2 introduces the registry and converts Python/Node to built-in descriptors.
R5 may add a declarative kernel section to project/environment sync. No new CLI
top-level command is added; execution still goes through `run/call`.
