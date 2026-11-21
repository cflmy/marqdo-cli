# ADR-0057: Trusted capability-handler registration

Status: **Accepted**
Date: 2026-11-21
Implements: RFC-EXT-0001 §7 (PR-05)
Extends: ADR-0011 (capability broker)
Scope: `crates/marqdo-host/src/capability.rs` (host-internal registry)

## Context

ADR-0011 introduced `CapabilityBroker` with two separate ideas but only one
implementation path:

* `decide()` answered "is this capability granted?" from `Capabilities` and a
  trusted vendor-policy allow-list; and
* `execute()` special-cased the single real core implementation (`fs.read`)
  inline, returning `capability.unavailable` for everything else.

That shape is why adding a capability looked like a one-line allow-list edit,
and it made "declared in policy" and "can actually execute" the same object.
RFC-EXT-0001 §7.1/§7.2 requires the chain:

```text
manifest requests capability
  -> operator/CLI grants it
  -> broker asks: is a trusted handler installed?
  -> handler re-checks owner/grant/scope/budget
  -> handler executes
  -> audit { identity, effect, decision, duration, trace_id }
```

A name in an allow-list must never be enough to execute, and a capability with
no executor must never look like an empty success.

## Decision

### 1. Two independent registries

`CapabilityBroker` keeps:

* `vendor_policy: BTreeSet<String>` — names the trusted host *policy* accepts
  (from `allow_vendor`); and
* `handlers: BTreeMap<String, HandlerEntry>` — names that have an explicitly
  registered trusted executor.

`HandlerEntry` additionally carries an optional exact trusted package binding
(`None` = authorized by the capability grant alone).

The two sets cannot substitute for each other:

* vendor capability without a policy entry -> `permission.denied`
  (declaration is not a grant; CAP-01);
* vendor policy entry without a handler -> `capability.unavailable` (CAP-02);
* core capability granted by `Capabilities` but without a handler ->
  `capability.unavailable` (CAP-12).

### 2. Handler interface

```rust
pub struct CapabilityRequest<'a> {
    pub package_instance: &'a str, // trusted identity from the host, not wire data
    pub capability: &'a str,
    pub payload: &'a [u8],
    pub trace_id: &'a str,
}

pub trait CapabilityHandler: Send {
    fn name(&self) -> &str;
    fn execute(
        &mut self,
        request: &CapabilityRequest<'_>,
        capabilities: &Capabilities,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, PluginError>;
}
```

The registry key is `handler.name()`, so the handler's declared capability can
never disagree with the name it is registered under (the RFC's "declared
protocol mismatch" rejection cannot occur by construction).

Registration is host-only; there is no wire path from a plugin manifest to this
API. `register_handler` installs a handler usable by any package holding a
grant; `register_package_handler(package, handler)` binds it to one exact
trusted package identity and denies every other claimed package (CAP-03, the
"trust the package the host loaded, not a self-reported name" rule of §7.4).

### 3. Registration validation

`register_handler` / `register_package_handler` reject:

| condition | stable code |
|---|---|
| empty name, surrounding whitespace, or not a core/vendor name | `capability.handler_invalid` |
| name under a reserved host prefix (`marqdo.`, `host.`) | `capability.handler_reserved` |
| name already registered (including the built-in `fs.read`) | `capability.handler_conflict` |

A rejected handler is never installed. The component that constructs the
broker is the trust boundary: only trusted host/embedder code can register, so
"untrusted source" is rejected by the absence of an API rather than a flag.

### 4. Fixed execution chain and audit

`execute()` is one function with one order:

1. `decide()` — `Capabilities`, vendor policy, request payload shape;
2. handler installed? — otherwise audit `unavailable`, return
   `capability.unavailable`;
3. package-bound handler owns the claimed package? — otherwise audit `denied`,
   return `permission.denied`;
4. handler executes under the broker `Budget` and the broker reply limit;
5. audit one entry with `package`, `capability`, `decision`
   (`granted`/`denied`/`unavailable`), reason and `duration_micros`.

`authorize()` remains the grant decision only (`granted`/`denied`); it never
executes and therefore cannot demonstrate an executor. This keeps the
authorization and execution questions separate and auditable.

The reply is bounded by `reply_limit` (default
`MAX_CAPABILITY_REPLY_BYTES`) and oversize replies are `resource.exhausted`
(CAP-06). Handlers also receive `&mut Budget`, so an embedder can wire the
per-run budget and handlers can charge fuel/output (tested). Response payloads
are never written to the audit log, so file contents and request secrets stay
out of it (CAP-09).

### 5. `fs.read` becomes the first standard handler

The former inline `fs.read` match arm moved into `FsReadHandler`, installed by
`CapabilityBroker::with_fs(fs)`. It:

* resolves the path with `ProjectFs::resolve` (absolute paths, `..`, empty
  paths rejected) **before** I/O;
* re-checks `Capabilities::check_read` against the normalized
  project-relative path at the execution point, so the grant is enforced on the
  same path the filesystem uses;
* reads through `ProjectFs` (regular file, UTF-8, project root, 8 MiB limit).

`with_fs` still installs exactly one handler; `capability_broker.rs` and
`host.rs` call sites are unchanged. Every other core capability and every
vendor capability without a registered handler keeps returning
`capability.unavailable`; nothing returns an empty `Ok` as a stand-in for
"implemented".

## RFC deviations (explicit)

1. **`EffectiveGrant` -> `Capabilities`; `ResourceBudget` -> `Budget`.** The
   RFC (§7.2) says these are design-interface names and the implementation must
   reuse the existing grant and limit structures instead of building a second
   permission system; that is exactly what happened.
2. **`CapabilityError` -> `PluginError`.** The capability bridge across the
   plugin/worker ABI already uses `marqdo_plugin_abi::PluginError`; reusing it
   avoids a second error taxonomy. Codes remain the stable contract.
3. **`trace_id` is plumbed through `CapabilityRequest` and settable with
   `with_trace_id`, but the host wiring is a follow-up.** `host.rs` does not set
   it yet, so audit entries from the current CLI plugin path have an empty
   trace id. Wiring it requires the run/session id from `ExecutionContext`,
   which is outside this PR's write scope.
4. **`resource.exhausted` preserves the broker code but not the runtime
   category.** `PluginError` kind `Capability` maps to
   `RuntimeError` category `PermissionDenied` in `host.rs::map_plugin_error`.
   The stable code is `resource.exhausted` (what CAP-06 asserts at the broker
   boundary); teaching the host mapping about a resource-limit plugin code is a
   follow-up because `host.rs` is outside this PR's scope.
5. **`CapabilityBroker` is no longer `Clone`** because it owns `Box<dyn
   CapabilityHandler>`. No in-tree caller cloned it; a future embedder that
   needs a clone should share the broker behind the same lock instead, which is
   also what the one-call-at-a-time ABI requires.

## Alternatives

1. Keep the closed match arm and add capabilities to it. Rejected: every
   vendor operation would need a core release and the broker would still have
   no separation between "granted" and "executable".
2. Treat a vendor policy entry as proof of an executor. Rejected: that is the
   exact "declaration becomes privilege" failure the RFC forbids; CAP-02 locks
   it out.
3. Register handlers by `(name, package)` strings only and trust the caller's
   package argument. Rejected: a wire-supplied package name must not authorize
   anything; package-bound registration pins the identity the host selected at
   load time.
4. Give every core capability an empty handler so `execute()` always returns
   `Ok`. Rejected: it converts "not implemented" into a silent success, the
   failure mode RFC §7.1 calls out.

## Compatibility

* Existing `Capabilities`, `with_fs`, `with_audit`, `allow_vendor`,
  `authorize`, `execute` and `capability_audit()` call sites are unchanged;
  `host.rs` needed no edit.
* One observable change: a vendor capability without a trusted policy entry is
  now `permission.denied` instead of `capability.unavailable`. This matches
  ADR-0011 ("a missing grant is `permission.denied`") and RFC CAP-01.
* `CapabilityAudit` gained `duration_micros`; all fields remain public and the
  existing tests that read `package`/`capability`/`decision` compile unchanged.
* `CapabilityBroker` lost `Clone` (see deviation 5).
* No ABI, wire, manifest or CLI change.

## Security

* Registration is a trusted-host operation with no manifest/wire entry point;
  a plugin cannot install, replace or enumerate handlers.
* Package-bound handlers reject cross-package impersonation even when policy
  and grant match.
* `fs.read` keeps `ProjectFs` root confinement, glob scope and size limits; the
  grant is re-checked on the normalized path at the execution point.
* Audit records decisions and durations but never payload contents.
* This remains a cooperative in-process API contract, not an OS sandbox: a
  native plugin can still issue direct syscalls. The registry narrows what the
  host will *do on request*; it does not confine plugin memory or syscalls.

## Testing

`crates/marqdo-host/tests/capability_handlers.rs` covers CAP-01..CAP-12:
policy-vs-handler separation, package binding, registration rejection, real
`fs.read` scope enforcement, reply budget and handler budget charging, forged
and replayed handles, audit hygiene, resource-stream regression smoke, project
mode not being OS-strict, and "no handler is never a silent success".
Unit tests in `capability.rs` keep the `CORE_CAPABILITIES` list in sync with
`decide()` and assert that the built-in `fs.read` handler is installed by
`with_fs`.

## Migration & rollout

`with_fs` installs the migrated `fs.read` handler automatically, so existing
plugin loads keep working. Future core capabilities migrate by adding a handler
and registering it in the trusted composition root; until then they remain
`capability.unavailable`. Vendor operations that need a non-streaming API
should first use the existing `resource.open/read/write/close` stream path
(RFC-EXT-0001 §7.3) instead of registering a new host handler.
