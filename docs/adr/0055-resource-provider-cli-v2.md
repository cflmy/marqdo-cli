# ADR-0055: Resource Provider v2 through the normal CLI

Status: **Accepted**
Date: 2026-11-21
Relates to: RFC-EXT-0001 §5/§16/§18, ADR-0033, ADR-0034, ADR-0035, ADR-0036,
ADR-0037, ADR-0038, ADR-0021 (strict L1), ADR-0052 (single Markdown semantics)
Specs: `spec/resource-provider-v2.md`
Implementation: `crates/cli/src/provider_manifest.rs`,
`crates/cli/src/commands/sandbox.rs`, `crates/cli/src/commands/run.rs`,
`crates/cli/src/main.rs`, `crates/cli/tests/provider_cli.rs`

## Context

The host already implements versioned resource-provider registration
(`register_resource_provider`, `register_worker_resource_provider`,
`register_c_resource_provider`, `register_provider_package`) and
`resource.open` dispatches unknown kinds through that registry with a provider
name grant (ADR-0033/0035/0036/0037/0038). What was missing was a **public
composition-root entry point**: `main.rs`/`SandboxSettings`/`run.rs` had no
`--provider`/`--allow-provider`, so an external repository could not use a new
resource kind on a frozen `marqdo` binary without patching and rebuilding the
CLI (RFC-EXT-0001 EXT-07).

The existing `marqdo.provider` manifest (`schema_version: 1`, ADR-0037) is a
host-embedding installation format: `name/kinds/abi/library`, no `version`, no
targeted artifact digest and no transport. It cannot support portable,
integrity-checked CLI loading. RFC-EXT-0001 §5.3 asks for a v2 document rather
than silently overloading v1. It also requires that selecting an extension is
not authorizing it, that `pure`/`strict` keep failing closed, and that no new
`HostOp`/builtin/business branch is introduced.

This ADR records the PR-03 decision on the current branch.

## Decision

### 1. CLI surface and ordering

`--provider PATH` (repeatable, a `provider.json` file) and
`--allow-provider NAME` (repeatable, exact name) are added to `SandboxArgs`, so
they are shared by `run` and `call` only. `check`/`inspect` do not declare them
and never read a provider manifest. `SandboxSettings` carries
`providers: Vec<PathBuf>` and `allow_provider: Vec<String>`; the `project`
capability set calls the existing `Capabilities::allow_resource_provider(name)`
for each grant.

Run ordering follows RFC-EXT-0001 §18.2: `prepare` (no provider work) →
diagnostics → mode/strict preflight → capabilities → host assembly and budgets
→ kernels → **all provider manifests/targets/artifacts/grants/duplicate kinds
verified and plugin artifacts verified** → plugin workers loaded and exports
reconciled → provider helpers spawned → entry-function check → execute. No
worker (plugin or provider) starts before every T6/T6b verification passes, and
the main function never runs after a provider failure.

`pure` and `strict` reject both flags in preflight (`cli.invalid_grants` /
`strict.unsupported_grant`, exit 2) without silently switching to `project`.
`--allow-provider` rejects wildcards (`*`, `?`, `[`, `]`) and empty names; a
grant that matches no selected manifest is `ext.provider_grant_unmatched`
(exit 2). A `--provider` selected without its exact grant is
`permission.denied` (exit 1) before the helper is spawned.

### 2. Strict v2 manifest parser

New `crates/cli/src/provider_manifest.rs` owns the document, separate from the
plugin `plugin.yaml` parser:

* accepts exactly `schema = "marqdo.provider"`, `schema_version = 2`,
  `abi = "c-abi-1"`, `transport = "worker"`; other versions/ABIs/transports are
  `ext.unsupported_provider_schema` (no silent migration, no v1 fallback from
  the CLI);
* strict dotted lowercase name/kind grammar, strict SemVer, non-empty unique
  non-reserved kinds, bounded counts, known/vendor `permissions_requested`
  declarations only, optional non-executable `metadata`;
* unknown top-level/artifact fields are rejected (`ext.manifest_invalid`);
* artifacts need unique parseable targets and 64-lowercase-hex sha256;
  the current `HostTarget` artifact is selected (`ext.no_matching_artifact`
  when absent), resolved inside the manifest bundle (absolute/`..`/symlink
  escape/non-regular/size limit/sha mismatch → `ext.artifact_integrity`);
* duplicate names with different manifest digests (`ext.duplicate_provider`)
  and duplicate kinds across providers (`ext.duplicate_provider_kind`) are
  deterministic because paths are canonicalised, sorted and de-duplicated
  before conflict detection.

`VerifiedProvider` is the static `{name, kinds, manifest_path, base_dir,
manifest_digest, transport, selected_artifact_path/relative/sha256/target,
version}` record. It is still declaration/verification only: no `dlopen`, no
spawn, no grant.

### 3. Activation reuses the existing host API

`load_resource_provider_into_host(host, verified, allow_provider)` repeats the
exact grant check, re-verifies the manifest digest and the artifact
(regular file, containment, size, sha256) immediately before use, resolves the
helper, and calls **the existing**
`StdHost::register_worker_resource_provider(name, kinds, helper, [artifact])`.
It never falls back to `register_c_resource_provider`,
`register_provider_package`, in-process native loading or another provider.
`resource.open` still re-checks the provider name grant in the Host. Provider
registration keeps using `register_provider_checked` for name/kind/duplicate
validation, so no second provider registry is introduced.

### 4. Helper resolution and platform claim

The helper is resolved only as a sibling of the running `marqdo` executable:
`<exe dir>/marqdo-provider-worker[.exe]`. `PATH`, the current directory,
environment overrides and downloads are not consulted. Missing/non-regular
helper, spawn failure or failed `ping` → `ext.worker_unavailable` (exit 1).

The `worker` transport is claimed only on **linux/x86_64**; every other target
returns `ext.platform_unsupported` (exit 1) instead of pretending success.
Release artifacts must ship the helper next to the CLI for the same target
triple. macOS/Windows stay buildable and explicitly unsupported until they have
a native fixture and release evidence (RFC-EXT-0001 §5.6).

### 5. No new host op or language surface

There is no new `HostOp`, builtin, `resource_request` string branch, `Value`
variant or Markdown syntax. A provider kind is still an argument to
`resource.open("<kind>", descriptor, rights)`; the only change is the CLI
composition root that selects, verifies and authorizes the existing host
transport. Provider tokens stay inside the helper; the language sees only the
opaque Host `Resource` handle. Worker errors are structured `RuntimeError`s
surfaced as CLI errors — the CLI never panics on a helper crash/timeout.

### 6. Tests

`crates/cli/tests/provider_cli.rs` drives the real CLI binary with a real C
`c-abi-1` sensor library and the real `marqdo-provider-worker` helper built by
cargo and installed next to the CLI (the same layout a release tarball uses;
production resolution is not weakened for tests). Coverage: PROV-01/02/03/04/
05/06/07/08/09/10/13, plus `check` refusing the flag. Marker files written by
the C library's ABI-version hook prove whether a worker actually started, so
"no worker on unauthorized/invalid input" is evidence, not an assertion about
code paths.

## Consequences

* A third party can add a new resource kind to a frozen `marqdo` binary through
  `provider.json` v2 + `--provider` + `--allow-provider`, without a core
  rebuild, new builtin or new CLI command.
* v1 `marqdo.provider` remains a host-embedding format; CLI users get a clear
  `ext.unsupported_provider_schema` instead of a silently unverified load.
* Integrity hashing raises the cost of accidental corruption/tampering but is
  not a trust root; a compromised or malicious helper still runs with the
  user's privileges. Documentation states this explicitly.
* `pure`/`strict` semantics and the existing resource/provider host contracts
  are unchanged (strict still L1-only and fail-closed).
* The provider helper becomes a release artifact that must be packaged next to
  the CLI; a missing helper is a first-class, deterministic error.

## Alternatives considered

1. **Reuse `plugin.yaml`/plugin ABI directly for providers.** Rejected: a
  resource provider's contract is open/read/write/close with provider-private
  tokens and stream kinds, not callable exports; sharing the manifest/ABI
  parser would conflate two independent protocols (RFC-EXT-0001 §1.3).
2. **Extend v1 in place with optional `version`/`artifacts`.** Rejected:
  v1 is host-embedding installation metadata and is already accepted by
  `register_provider_package`; silently changing its meaning or accepting a
  half-populated v1 would break the frozen ADR-0037 contract. v2 is explicit.
3. **Allow in-process C ABI loading when the worker helper is missing.**
  Rejected: it would silently lower the process boundary and RFC-EXT-0001
  §5.2/§5.4 forbids falling back to Native. `ext.worker_unavailable` is the
  correct fail-closed result.
4. **PATH lookup for `marqdo-provider-worker`.** Rejected: `PATH` is
  attacker-controlled state and would make the executed artifact
  non-deterministic; the sibling rule binds the helper to the release.
5. **Register providers lazily at the first `resource.open`.** Rejected for
  this slice: authorization and integrity must be decided before the main
  function executes, and the existing host API is synchronous spawn-on-register.
6. **Accept a `*` provider grant for developer convenience.** Rejected: grants
  are exact names (RFC-EXT-0001 §5.2); wildcards would let a typo authorize an
  arbitrary selected provider.

## Deviations recorded

* `ext.manifest_invalid`, `ext.duplicate_provider` and
  `ext.provider_grant_unmatched` are **additive** `ext.*` codes beyond the list
  frozen in RFC-EXT-0001 §18.1. The frozen list has no generic malformed-v2
  code, no duplicate-provider-name code and no unmatched-grant code; reusing
  `ext.unsupported_provider_schema`/`ext.duplicate_provider_kind` for these
  would report the wrong reason. They follow the same `ext.` naming and exit
  code contract and are frozen by this ADR.
* `ext.artifact_integrity` failures exit 1 (load/verify), while static
  manifest-selection failures exit 2, matching RFC-EXT-0001's exit-code
  guidance; plugin artifact checking already uses the same split.
* Only linux/x86_64 is claimed for the provider `worker` transport in this
  release (RFC-EXT-0001 §5.6 permits this explicit platform limit; Windows and
  macOS return `ext.platform_unsupported`).

## Verification

`crates/cli/tests/provider_cli.rs` (12 real tests, linux/x86_64) plus
`crates/cli/src/provider_manifest.rs` unit tests pass; `spec/security.md`,
`spec/resource-provider-v2.md` and the README provider section document the
same contract. Evidence is recorded in the PR-03 task report (commands, exit
codes, per-PROV matrix).
