# ADR-0014: Plugin handles are host-minted authority, never wire claims

Status: **Accepted**
Date: 2026-11-20

## Context

`marqdo_value::PluginHandle { plugin, export, id }` is decodable from the
public wire format. M0.5 documented it as non-unforgeable but had no host table
and no producer; a future stateful plugin API could accidentally treat a
decoded JSON handle as authority. The M0.6 audit E15 requires fixing the
authority model **before** stateful plugin resources are added.

## Decision

`marqdo-host::PluginHandleTable` owns every live handle:

```rust
struct HandleEntry {
    plugin: String,
    export: String,
    owner_package: String,
    run_id: u64,
    generation: u64,
}
```

* Handles are minted only by the host (`mint`) and carry the owner package and
  run id; `validate` requires an exact plugin/export/id match plus the caller's
  package and current run.
* `clear_run` invalidates every handle from a run; generation increments on
  mint/clear, so an old handle cannot be revived.
* A plugin or foreign kernel returning a `PluginHandle` in its typed output is
  rejected (`plugin_handle.untrusted`), including handles nested in lists,
  maps or tables. Untrusted output may not mint host authority.
* Wire JSON remains able to represent a handle for data/debugging, but decoding
  is not authorization. The runtime must call `validate` with the trusted
  context before any future resource operation.

No stateful plugin resource API is added in this milestone; the table and the
boundary rejection exist so that such an API cannot be built on a forgeable
value.

## Alternatives

1. Remove `PluginHandle` from the wire entirely. Rejected for now: it is part
   of the frozen v1 value union; rejecting untrusted output at the host achieves
   the required authority separation without a wire break.
2. Use the `id` alone as a capability token. Rejected: IDs are guessable and
   cross-session replay would work.
3. Sign handles cryptographically. Rejected as unnecessary complexity when a
   host-side table with run/owner scope is available.

## Compatibility

The wire encoding is unchanged. Existing values remain decodable; a handle
returned by a plugin/foreign kernel now fails at the host boundary instead of
being surfaced as an inert value. No legitimate M0.5 producer exists (none was
implemented), so this is not a behavioral regression.

## Security

The table is in host memory and never serialized. A forged, cross-package,
cross-run or post-unload handle fails. The host address and internal entry
fields are not exposed by `PluginHandle`.

## Testing

`crates/marqdo-host/src/handles.rs` tests forged IDs, wrong export, cross-owner,
cross-run and cleared-run rejection, plus nested-handle detection. The plugin
and foreign adapters reject untrusted handle output.

## Migration & rollout

R2 adds the table and boundary rejection. A future stateful API must define
resource lifetime, owner transfer rules and a host op that validates the table;
it must not accept handles straight from wire JSON.
