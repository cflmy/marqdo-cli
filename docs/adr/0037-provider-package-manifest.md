# ADR-0037: Provider package manifest v1

Status: **Accepted (host-side installation metadata)**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md`
(CM-A09, CM-A10, P5), ADR-0036
Specs: `spec/evaluation.md`, `spec/security.md`
Evidence: `docs/evidence/2026-11-20-provider-package.md`

## Context

ADR-0036 lets a trusted C shared library implement resource operations, but the
host still needed a code-level path to the library. CM-A09 asks for provider
installation/discovery without changing the parser/runtime. This ADR defines a
small, versioned **package manifest** so a provider distribution can be
installed by a host embedding and reused by future external provider
repositories.

## Decision

### 1. Manifest format

```json
{
  "schema": "marqdo.provider",
  "schema_version": 1,
  "name": "cabi",
  "kinds": ["cabi.sensor"],
  "abi": "c-abi-1",
  "library": {
    "linux": "libcabi_provider.so",
    "macos": "libcabi_provider.dylib",
    "default": "libcabi_provider.so"
  }
}
```

* file name: `provider.json` by convention; the host API takes an explicit
  manifest path.
* `schema`/`schema_version` must be exactly `marqdo.provider`/`1`.
* `abi` must be `c-abi-1` (ADR-0036); future ABIs need a new ADR.
* library paths are relative to the manifest directory (absolute paths are
  accepted only for explicit local tests); platform key is `linux`, `macos`,
  `windows`, or `default`.
* unknown fields are tolerated by serde, but unknown schema/ABI are rejected.

### 2. Host API and authorization

* `StdHost::register_provider_package(manifest_path)` reads/validates the
  manifest, loads the C ABI library and registers it through the same
  kind/name validation as ADR-0033/0036.
* Registration does not authorize language use: `resource.open` still requires
  project mode and `Capabilities.allow_resource_provider(name)`. Pure/strict
  refuse provider kinds.
* Language imports cannot read or install a provider manifest. Installation is
  a host/package-manager action (INV-06).

### 3. Trust boundary

* The manifest points at trusted native code loaded in-process; it is not a
  sandbox and does not add signatures. A future provider repository must add
  signatures/verification and a Worker binding.
* No network/package registry fetch is implemented here; the host passes a
  local path.

## Non-goals

* Provider registry download, signatures, dependency resolution or semver.
* CLI install/uninstall commands (host embedding only).
* Untrusted-provider isolation or strict-mode provider support.
* Automatic platform selection beyond the `library` map.

## Verification

`docs/evidence/2026-11-20-provider-package.md` records the manifest round-trip
test: compile a C provider, write `provider.json`, `register_provider_package`,
then `resource.open/read`.
