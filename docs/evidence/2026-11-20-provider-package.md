# RFC-CM CM-A09 evidence: provider package manifest v1

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `5e5c3a8` plus this step's changes.
Decision record: [ADR-0037](../adr/0037-provider-package-manifest.md)
Related: [C ABI provider](2026-11-20-c-provider.md), [STATUS §5](../STATUS.md).
Scope: local manifest installation; no registry/signature/Worker binding.

## Manifest

```json
{
  "schema": "marqdo.provider",
  "schema_version": 1,
  "name": "cabi",
  "kinds": ["cabi.sensor"],
  "abi": "c-abi-1",
  "library": {"linux": "libecho_provider.so", "default": "libecho_provider.so"}
}
```

`StdHost::register_provider_package` validates schema/version/ABI, resolves the
platform library relative to the manifest, loads it through the ADR-0036 C ABI
adapter and registers it through the shared kind/name checks. Language use still
requires project mode and `allow_resource_provider("cabi")`.

## Test

`crates/marqdo-host/tests/c_provider.rs::provider_package_manifest_installs_a_c_abi_library`:

1. compile the C provider with `cc -shared`;
2. write `provider.json` next to it;
3. `register_provider_package(&manifest)` succeeds;
4. `resource.open("cabi.sensor", ...)` and `resource.read` return `c-`.

Existing C tests cover version mismatch, missing library and pure/strict refusal.
No parser/`Value`/runtime/`HostOp` change was needed.

## Not done

* Provider registry/download, signatures, dependency resolution, semver.
* CLI install/uninstall commands; installation is host embedding API only.
* Untrusted-provider isolation, strict-mode providers, plugin-Worker binding.
* Windows library build lane.
