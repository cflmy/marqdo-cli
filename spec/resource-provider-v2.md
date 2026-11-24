# Marqdo Resource Provider v2 (`provider.json`)

Status: **frozen provider manifest schema version 2** for the `marqdo` CLI
composition root. Implementation: `crates/cli/src/provider_manifest.rs`;
prerequisite host transports: ADR-0033/0035/0036/0037/0038; CLI decision:
ADR-0055. The worker helper is `marqdo-provider-worker`
(`crates/marqdo-provider-worker`, ADR-0038).

> **A trusted helper is not a sandbox.** The provider `worker` transport runs a
> separate process with the invoking user's OS privileges. It isolates a crash
> or protocol failure from the interpreter; it does **not** restrict file
> system, network, process or memory access. `sha256` values are content
> *integrity* checks, not publisher authentication. `strict` L1 refuses every
> provider (fail-closed) until a real isolation adapter exists.

## 1. CLI surface

```text
marqdo run  FILE [--provider PATH/provider.json ...] [--allow-provider NAME ...]
marqdo call FILE FUNCTION [--arg NAME=VALUE ...] \
             [--provider PATH/provider.json ...] [--allow-provider NAME ...]
marqdo check FILE            # no --provider/--allow-provider options exist
```

* `--provider PATH` is repeatable. `PATH` names a `provider.json` file
  (directories are rejected). Only explicitly passed manifests are considered;
  there is no directory scan, package-manager discovery, registry download or
  network fetch.
* `--allow-provider NAME` is repeatable and matches the manifest `name`
  **exactly**. There is no `*`/glob wildcard, prefix match or implicit
  authorization. Selecting a provider with `--provider` never grants it.
* `check`/`inspect` do not accept these options and never read a provider
  manifest, start a helper or call provider code. Provider kinds are not new
  callables; they are values passed to `resource.open(kind, ...)`.
* `--mode pure` and `--mode strict` reject `--provider`/`--allow-provider` in
  preflight (exit 2, no helper spawn, no silent downgrade to `project`).
  `pure` reports `cli.invalid_grants`; `strict` reports
  `strict.unsupported_grant`.

### 1.1 Ordering

`run`/`call` follow RFC-EXT-0001 §18.2:

1. T1–T5: load the Markdown graph and prepare call bindings (unchanged;
   providers are not part of `prepare`).
2. T6: parse all `--provider` manifests, validate schema/name/version/ABI/
   transport/kinds/artifact declarations, select and verify the current-target
   artifact (regular file, inside the bundle, size bound, sha256), reject
   duplicate provider names and duplicate kinds across providers, and check
   the exact `--allow-provider` grants.
3. T6b: verify all selected plugin manifests/artifacts (existing behavior).
4. T7: start plugin workers and reconcile exports, then spawn one
   `marqdo-provider-worker` per granted+verified provider.
5. T9: execute the entry function; every `resource.open` re-checks the provider
   name grant in the Host.

No worker process (plugin or provider) is started before every T6/T6b check
passes. A manifest, target, integrity, grant or duplicate-kind failure leaves
the main function unexecuted.

An `--allow-provider NAME` that matches no selected manifest is a deterministic
option error (`ext.provider_grant_unmatched`, exit 2) rather than silent
discovery fan-out.

## 2. Manifest file

* File name convention: `provider.json`. It must be a UTF-8 JSON object of at
  most 1 MiB and is read from an explicit path, never from `PATH`.
* The CLI accepts **schema version 2 only**. A `schema_version: 1` manifest is
  rejected with `ext.unsupported_provider_schema`; there is no silent
  migration and no v1/v2 auto-detection. (v1 remains usable only through the
  host embedding API `StdHost::register_provider_package`, ADR-0037.)
* Unknown top-level fields are rejected inside schema v2. Extensible,
  non-executable data must live in the `metadata` object, which has no
  execution meaning. A future schema version must be a new document, not an
  in-place extension of v2.

```json
{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "path": "lib/libdemo_sensor.so",
      "sha256": "<64-lowercase-hex-sha256>"
    }
  ],
  "permissions_requested": []
}
```

### 2.1 Fields

| Field | Required | Rules |
|---|---|---|
| `schema` | yes | Exactly `"marqdo.provider"`. |
| `schema_version` | yes | Exactly `2` (integer). |
| `name` | yes | Stable grant name. Non-empty dotted name: each `.`-separated segment starts with a lowercase ASCII letter and continues with lowercase ASCII letters, digits, `-` or `_`; at most 128 bytes. A name equal to a reserved core resource kind is rejected. |
| `version` | yes | Strict SemVer (`semver::Version`, three numeric components). |
| `abi` | yes | Exactly `"c-abi-1"` (ABI v1, ADR-0036/0038). Anything else is `ext.unsupported_provider_schema`. |
| `transport` | yes | Exactly `"worker"` (framed-JSON helper, ADR-0035/0038). Anything else is `ext.unsupported_provider_schema`. |
| `kinds` | yes | Non-empty, at most 64 unique dotted names with the same grammar as `name`. Core kinds are reserved (see §2.2). |
| `artifacts` | yes | Non-empty, at most 32 entries with unique target triples. Each entry has `target`, `path`, `sha256`; unknown entry fields are rejected. |
| `permissions_requested` | no | Declarations only, at most 64 unique strings: a known host capability (`marqdo_plugin_abi::KNOWN_CAPABILITIES`) or a dotted vendor capability (at least three segments, no core namespace prefix). Declarations never grant anything. |
| `metadata` | no | Arbitrary JSON. Ignored for execution; never used to select artifacts or grant permissions. |

`artifacts[].target` must parse as a target triple. `artifacts[].path` is
bundle-relative: absolute paths, `..` components and other escapes are
rejected. `artifacts[].sha256` must be 64 lowercase hexadecimal characters.
`artifacts[].path` is resolved against the manifest's own directory (the
bundle root); a symlink is followed only when the canonical target stays inside
that root, and the final target must be a regular file no larger than 256 MiB.

### 2.2 Reserved core kinds

`stdio`, `file`, `tcp`, `tls`, `udp` and `tcp-listener` are implemented by
`marqdo-host` before the provider registry is consulted. A provider may not
declare them (nor use one as its `name`); registration can never shadow them.

Reserved core kinds are rejected with `ext.manifest_invalid`. The list is the
single host-owned constant `marqdo_host::CORE_RESOURCE_KINDS`; the CLI manifest
validator and trusted `Host::register_resource_provider` both use it and reject the
same six names before registration (ADR-0062).

### 2.3 Artifact selection

The only artifact used is the one whose `target` equals
`marqdo_plugin_abi::HostTarget::current().triple()`. A missing current target
is `ext.no_matching_artifact` (exit 2); another platform's artifact or a
`default` fallback is never substituted.

## 3. Identity and integrity

* The manifest content digest is `sha256:<hex>` over the manifest bytes; the
  selected artifact digest is the declared lowercase `sha256`. Both are
  re-verified immediately before the helper is spawned, and again when the
  registration helper is called, so a file replaced between preparation and
  activation fails closed.
* These hashes prove the manifest/artifact pair is byte-identical to what was
  selected. They do **not** prove who published it, that it is trustworthy or
  that it may be granted. Publisher trust needs a signature/trust-root layer
  that is out of scope for schema v2.
* No version of this schema stores a secret, environment value or shell
  fragment. Manifests are never concatenated into a shell command.

### 3.1 Duplicate identity

* The same provider `name` selected with two different manifest digests is
  `ext.duplicate_provider`; the exact same manifest selected twice is
  idempotent.
* One non-reserved `kind` declared by two providers is
  `ext.duplicate_provider_kind`. Registration never overwrites a previous
  provider ("last wins" is forbidden).
* Error selection is deterministic: manifests are canonicalised, de-duplicated
  and sorted before conflicts are reported, independent of CLI argument order.
* One provider may serve several non-reserved kinds; all of them are
  registered together and must be usable.

## 4. Worker helper resolution

The CLI resolves the helper **only** as a sibling of the running `marqdo`
executable:

```text
<directory of the marqdo executable>/marqdo-provider-worker[.exe]
```

* No `PATH` lookup, no current-directory search, no environment override and no
  download fallback. A helper found on `PATH` is ignored.
* A missing sibling (or a non-regular file) is `ext.worker_unavailable`
  (exit 1). A helper that fails to start or fails the protocol ping is also
  `ext.worker_unavailable`; there is no fallback to in-process/native loading.
* Release archives must ship the `marqdo` binary and the
  `marqdo-provider-worker` helper built for the same target triple.
* The `worker` transport is supported only on **linux/x86_64** in this release.
  Other platforms compile but fail closed with `ext.platform_unsupported`
  (exit 1). Windows and macOS are explicitly unsupported until they have a
  native fixture and release evidence; Linux success is not a platform claim.

## 5. Worker protocol v1 and token lifecycle

The helper is `marqdo-provider-worker <selected-artifact-path>`. It loads
exactly one `c-abi-1` shared library, verifies
`marqdo_resource_provider_abi_version() == 1`, and speaks the ADR-0035 framed
protocol over stdin/stdout: 4-byte big-endian length + JSON, 1 MiB maximum per
frame. Framing and process plumbing come from the
[shared worker transport](worker-ipc.md); the operation vocabulary, kind rights
and token lifecycle below remain owned by this boundary.

```text
{"op":"ping"}                              -> {"ok":true}          (handshake)
{"op":"open","kind","descriptor","rights"} -> {"ok":true,"token":u64}
{"op":"read","token","max_bytes"}          -> {"ok":true,"bytes":[u8...],"eof":bool}
{"op":"write","token","bytes":[u8...]}     -> {"ok":true,"count":u64}
{"op":"close","token"}                     -> {"ok":true}
{"op":"shutdown"}                          -> process exit
{"ok":false,"code","message"}              -> structured failure
```

Token lifecycle:

1. `ping` must succeed before the host registers the provider. A failed
   handshake aborts the run (`ext.worker_unavailable`).
2. `open` creates a provider-private `u64` token in the helper. The language
   never sees it: the Host issues the opaque language `Resource` handle that
   binds run id + provider name + token. Provider tokens cannot be forged into
   language values or wire messages.
3. `read`/`write`/`close` are dispatched by the Host with the token it stored.
   A read returns at most `max_bytes`; bytes cross the boundary by copy.
4. `close` closes exactly one token. Repeated or unknown tokens must fail with
   a structured error; the Host never panics.
5. When the run ends (normal, error or timeout) every open token is closed and
   the helper receives `shutdown`; the Host kills and reaps the child on drop.
   A helper crash/protocol violation becomes a structured `host.error` for the
   offending operation (CLI exit 1) and is never retried against another
   provider or an in-process transport.
6. Provider error codes map to existing categories: `permission.denied`,
   `resource.closed`, `resource.exhausted`, `resource.unsupported`, otherwise
   `host.error`.
7. Any transport failure — including a request frame that exceeds the 1 MiB
   budget or a malformed reply — kills and reaps the helper. The peer is never
   reused after a protocol error, so a program that sends an oversized request
   must expect subsequent calls to that provider to fail closed rather than
   silently truncate the payload.

## 6. Authorization

* Registration, manifest parsing and artifact verification are not
  authorization. Every `resource.open("<kind>", ...)` still requires:
  project mode, the provider name in `Capabilities.resource_providers`
  (populated only by exact `--allow-provider NAME`), and the Host's provider
  name check. `permissions_requested` declarations do not affect this.
* `pure` and `strict` never reach provider kinds. `project` is the only mode
  that can use a third-party provider.
* Unauthorized selection fails closed before the worker is spawned:
  `--provider` without a matching `--allow-provider` is `permission.denied`
  (exit 1) and the helper is never started.

## 7. Error codes and exit codes

Manifest/CLI option failures exit 2; load/verification/spawn failures exit 1;
`permission.denied` exits 1. With a machine destination the CLI publishes
exactly one terminal event: a `run.rejected` event whose `payload.error.code`
carries the same code.

| Code | Stage | Trigger |
|---|---|---|
| `ext.manifest_unreadable` | manifest read (2) | missing/unreadable path, directory instead of `provider.json` |
| `ext.unsupported_provider_schema` | manifest read (2) | `schema` mismatch, `schema_version != 2`, unsupported `abi`/`transport` |
| `ext.manifest_invalid` | manifest read (2) | invalid UTF-8/JSON, unknown field, bad name/kind/version/permission, reserved kind, duplicate kind/target inside one manifest |
| `ext.no_matching_artifact` | manifest read (2) | no artifact for the current host target |
| `ext.duplicate_provider` | manifest read (2) | same provider name with two different manifest digests |
| `ext.duplicate_provider_kind` | manifest read (2) | two providers declare the same non-reserved kind |
| `ext.provider_grant_unmatched` | grant check (2) | `--allow-provider` names no selected manifest |
| `ext.artifact_integrity` | artifact verify / pre-spawn re-verify (1) | sha256 mismatch, absolute/`..`/symlink escape, non-regular file, size limit, manifest changed after verification |
| `permission.denied` | grant check (1) | `--provider` selected without its exact `--allow-provider` grant |
| `ext.worker_unavailable` | spawn (1) | sibling helper missing/not a regular file, spawn failure, ping failure |
| `ext.platform_unsupported` | spawn (1) | provider `worker` transport on a non-linux/x86_64 build |
| `cli.invalid_grants` | CLI preflight (2) | wildcard/empty `--allow-provider`, provider flags in `pure` mode |
| `strict.unsupported_grant` | strict preflight (2) | provider flags in `strict` mode |

`ext.manifest_invalid`, `ext.duplicate_provider` and
`ext.provider_grant_unmatched` are additive codes: RFC-EXT-0001 §18.1 froze the
provider-specific codes it could name, but a generic malformed-v2-manifest
code, a duplicate-provider-name code and an unmatched-grant code are required
to keep failure reasons distinct. They follow the same `ext.` naming and exit
code contract and are stable.

## 8. Test matrix (PR-03)

`crates/cli/tests/provider_cli.rs` builds a real C `c-abi-1` sensor provider,
runs the real `marqdo-provider-worker` next to the real CLI binary, and covers:

| ID | Evidence |
|---|---|
| PROV-01 | granted v2 worker + `resource.open/read/close("demo.sensor")` returns count 5, EOF, one close, `resource.open` in the host trace |
| PROV-02 | `--provider` without grant → `permission.denied`; marker proves the helper was never started |
| PROV-03 | `--allow-provider` without `--provider` → `ext.provider_grant_unmatched`, no worker |
| PROV-04 | same kind from two providers → `ext.duplicate_provider_kind` before any worker |
| PROV-05 | one provider with two non-reserved kinds → both open/read/close, one helper |
| PROV-06 | `stdio`/`file`/`tcp`/`tls`/`udp`/`tcp-listener` kinds rejected, no worker |
| PROV-07 | digest mismatch, symlink escape, `..` traversal → `ext.artifact_integrity`, no worker |
| PROV-08 | no current-target artifact → `ext.no_matching_artifact` |
| PROV-09 | `schema_version: 1` → `ext.unsupported_provider_schema` |
| PROV-10 | `pure`/`strict` + provider flags → preflight refusal, no worker; wildcard grant rejected |
| PROV-13 | relocated CLI without sibling helper (helper present on `PATH`) → `ext.worker_unavailable`, PATH is never used |

PROV-11 (worker crash/timeout/size-limit), PROV-12 (closed/stale/forged token),
PROV-14 (concurrent lifecycle) and PROV-15 (frozen-binary external repository)
are covered by the host worker/protocol tests or belong to the PR-07 external
conformance repository; they are not part of this CLI slice's manifest matrix.
