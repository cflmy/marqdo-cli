# ADR-0011: Capability broker and vendor-namespaced requests

Status: **Accepted**
Date: 2026-11-20

## Context

M0.5 has a useful `Capabilities` struct but no way for an extension to request
a capability at runtime:

* `marqdo-plugin-abi::KNOWN_CAPABILITIES` is a closed list and unknown names are
  rejected during manifest validation;
* `Host::load_plugin` passes `LoadOptions::default()`, whose capability handler
  is empty, so every plugin `request_capability` is denied;
* adding a new capability requires changing the core enum, the manifest list and
  the host dispatch together.

The M0.6 audit (E04/E09/E13) requires a broker: core capabilities stay finite
and auditable, while vendor-namespaced requests are declarative and can execute
only when a trusted host policy provides a handler and the user grants them.

## Decision

### 1. Two capability classes

**Core capabilities** are implemented by the host and keep a fixed list:
`console`, `math.compute`, `fs.read`, `fs.write`, `net.request`, `process`,
`clock`, `random`, `log`, `artifact.record`. They map to concrete host
operations and existing CLI grants.

**Vendor capabilities** match `^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)*$`
and must contain a vendor namespace (for example `vendor.device.read`). A
manifest may declare them, but a declaration is only a request. They never map
to a core operation automatically and cannot spell a core capability with a
different prefix (`fs.read` cannot be recreated as `vendor.fs.read`).

### 2. Broker interface

```rust
pub struct CapabilityRequest<'a> {
    pub plugin_instance: &'a str,
    pub package: &'a str,
    pub capability: &'a str,
    pub call_context: &'a ExecutionContext,
    pub reason: Option<&'a str>,
}

pub trait CapabilityBroker {
    fn authorize(&mut self, request: &CapabilityRequest<'_>) -> Result<CapabilityGrant, RuntimeError>;
    fn audit(&mut self, request: &CapabilityRequest<'_>, decision: &CapabilityDecision);
}
```

The CLI installs a broker populated from explicit user grants. The default
broker denies everything not in the effective `Capabilities` set. A vendor
capability is granted only if:

1. a trusted host policy has registered a handler for that exact name;
2. the user explicitly granted it for that package/instance; and
3. the request carries a live `ExecutionContext` (module, span, run id,
   generation).

A missing handler is `capability.unavailable`; a missing grant is
`permission.denied`. Both are distinct from runtime errors.

### 3. Budget, audit and scope

Every broker decision records the plugin instance, package, capability, run id
and decision in the effect trace. Grants are scoped to one run and one plugin
instance; they are not reusable across sessions, after unload, or by another
package. Resource limits (fuel, stack, timeout, output) are applied by the host
around the handler, not by the plugin.

### 4. ABI and manifest

The ABI callback signature is unchanged for v1. The loader passes a broker-backed
handler instead of `None`. Unknown **vendor** capability names are accepted by
manifest validation and reported as "requested, requires policy"; unknown core
names are rejected. No vendor capability can bypass `HostOp` dispatch: a
handler must ultimately call a host primitive and is subject to the same budget
and audit checks.

## Alternatives

1. Keep the closed `KNOWN_CAPABILITIES` list and add names as new core
   releases. Rejected: every vendor extension would require a core release.
2. Allow arbitrary strings and treat unknown names as granted. Rejected:
   privilege escalation by declaration.
3. Let plugins implement their own sockets/processes. Rejected: bypasses the
   host and the audit trail.
4. Use the existing `Capabilities` struct only. Rejected: it cannot express
   vendor handlers or per-instance/session scope.

## Compatibility

Core capability names and CLI flags remain valid. Manifests that previously
failed with `unknown_capability` for a valid vendor name now validate but still
fail at execution unless a trusted handler and grant exist. ABI v1 layout is
unchanged; future broker fields require an ABI minor/major negotiation.

## Security

The broker is the only path from an extension request to a host primitive. It
checks the actual package instance, not a self-declared name. It records audit
events and cannot mint capabilities from strings. Native in-process plugins are
still trusted code; the broker limits what they can ask the host to do, not what
they can do with their own memory.

## Testing

* authorized plugin reads a project-limited file; unauthorized read/write/
  process requests fail with `permission.denied`;
* a vendor capability without a handler fails with `capability.unavailable`;
* a grant cannot be reused by another plugin instance or after unload;
* a manifest cannot declare `fs.*` under a vendor prefix to obtain core access;
* audit entries contain the expected package/capability/decision.

## Migration & rollout

R2 implements the broker and vendor-name validation. `spec/security.md` and
`spec/plugin-abi.md` are updated. Existing plugins that only declare core
capabilities keep the same denied-by-default behavior.
