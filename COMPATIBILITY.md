# Compatibility and support matrix

Snapshot: product version **`3.0.0-beta.1`** (prerelease), branch `master`,
tagged `v3.0.0-beta.1` at source SHA `2c7c4f31e7a388d3fcfa717a77067e696ed13854`. This file records what was actually verified and
separates it from what is only implemented. The supported release platform is
**Linux x86_64 (glibc)**; everything else is experimental and not claimed for
this beta. Executed status and same-SHA evidence are in
[`docs/STATUS.md`](docs/STATUS.md), and SHA-bound external evidence is under
[`docs/evidence/extension/`](docs/evidence/extension/README.md).

The `3.0.0-rc.1` candidate targets Linux x86_64 on glibc 2.39 and
includes the post-beta changes below. See the
[RC acceptance procedure](docs/releasing.md). The platform evidence table
records the beta and subsequent development verification.

## Language source syntax (breaking change)

**The `.mq.md` source language is a breaking change.** The current contract is
[RFC-LANG-0002](docs/architecture/RFC-LANG-0002-unified-semantics.md)
(accepted by [ADR-0052](docs/adr/0052-unified-markdown-semantics.md)), with the
normative summaries in [`spec/mapping.md`](spec/mapping.md) and
[`spec/expression.ebnf`](spec/expression.ebnf):

- H1–H6 headings declare `@exec`/`@knowledge` modes; unmarked headings inherit
  the nearest ancestor's mode and the document root is Knowledge.
- Only an Exec H2 is a callable function. H2 Knowledge is inert and H3–H6 are
  non-callable sections.
- In an Exec scope, `Strong`, `Emph` and inline `Code` are interchangeable
  carriers of one statement; the original emphasis role is gone.
- Control flow comes from keywords in the payload (`if`/`else`/`while`/`end`/
  `return`/`params(...)`). Ordered lists, bullet lists and block quotes are
  inert presentation containers.
- There is no version selector, `--legacy` path or fallback mapper.

Consequences for compatibility:

- **Old pre-RFC `.mq.md` source is not executable.** A file that relied on
  `Strong` = execute, `Emph` = return, ordered list = branch, bullet list =
  loop, block quote = call or a first code-span signature must be rewritten by
  AST role (see `docs/migration/README.md`). A parse failure is a hard
  diagnostic, never a fallback to prose.
- The Markdown parser profile stays `marqdo.md.v1`; only the Markdown→program
  mapping changed. Rendering of otherwise-ordinary Markdown is unaffected.
- **Implementation state:** RFC-LANG-0002 is implemented by the frontend,
  expression parser, IR lowering, runtime and bundled stdlib. At
  `9c1723d` the workspace gate is
  `cargo test --workspace --locked`: 507 passed, 0 failed, 2 ignored, and the
  stdlib digest is regenerated and verified. See
  [`docs/STATUS.md`](docs/STATUS.md) and
  [`docs/evidence/2026-11-21-semantics-v2-green.md`](docs/evidence/2026-11-21-semantics-v2-green.md).

## Post-`3.0.0-beta.1` boundary changes (RFC-EXT-0003)

These landed after the beta tag and **are included in the `3.0.0-rc.1` candidate**; they are
intentional beta-line incompatibilities. Frozen decisions: ADR-0060..0063; executed
evidence: [`docs/evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md`](docs/evidence/2026-11-22-rfc-ext-0003-core-hardcoding.md).

| Change | Compatibility rule |
|---|---|
| `math.parse/eval/substitute/simplify/differentiate` removed from the core | **Breaking.** A call is a static `symbol.unknown`; there is no old-name parser, no `math.unsupported` runtime fallback and no ABI/Builtin shim. `Formula` stays an inert Markdown resource. Migrate to a pure `.mq.md` package or an explicitly selected ABI backend; `spec/math.md`. The `math` import namespace is released (no builtin conflict). |
| Sibling heading mode inheritance corrected (H-06) | **Behaviour fix.** Unmarked headings inherit the nearest ancestor, never the previous sibling. Documents that relied on the buggy sibling leak change: a former accidental Exec sibling can become inert Knowledge (or vice versa). Covered by conformance/black-box/`check --emit` tests; no syntax/version change. |
| Release archive must ship the verified `stdlib/` | **Delivery requirement.** Archive contains `stdlib.json` + all modules and `RELEASE_INFO.json` carries its name/version/tree/alias digests/count/files. Removing or tampering with it fails closed; release builds do not fall back to the build-machine source path. |
| Core resource kinds enforced on every registration path (H-04) | Provider name or declared kind equal to `stdio/file/tcp/tls/udp/tcp-listener` is rejected with `ext.manifest_invalid` (CLI) / `ResourceUnsupported` (host) before registration. No provider may shadow a core kind. |
| `resource_request` operation ownership (H-03) | New domain/business operations are forbidden in the core: ship a pure `.mq.md` package, ABI export or Provider. The exact reviewed allowlist is pinned by `architecture_boundaries.rs`; see `docs/architecture/resource-operations.md`. |
| Heading Annotations v1 (H-05/H-07) | **Proposed, not implemented.** This build still rejects unknown `@foo` with `sem.invalid_mode_marker`; no annotation manifest, `annotation.unknown` or heading annotations projection exists yet. Target `3.1.x`; ADR-0060 and `spec/heading-annotations-v1.md`. |
| Foreign removal F-B (H-02) | **Deferred, not claimed.** The one-shot permission-gated `foreign.*` primitive remains (F-A); its grants/digest/timeout/wire/output negatives are unchanged and must not be skipped. ADR-0063 and `spec/foreign.md` §8. |

## Structured control flow (RFC-CTRL-0005, unreleased)

The development branch reserves `break` and `continue` only as complete
payloads. An existing standalone identifier read can be written as
`(break)` or `(continue)`. Calls, bindings, fields and identifiers within
expressions retain their meaning. Two separate carriers containing `else` and
`if` still require two closing `end` statements.

Parameter inference ignores reads after a definite transfer. When upgrading
a caller that passed an argument inferred solely from unreachable code,
remove the argument or keep it through an explicit `params(...)` declaration.

## Public protocol versions

| Surface | Version | Compatibility rule |
|---|---|---|
| Markdown profile | `marqdo.md.v1` | Frozen parser/options. The RFC-LANG-0002 mapping rewrite keeps this profile; the source-language break is tracked above, not by a new profile id. |
| Language / CLI | `3.0.0-rc.1` (candidate) | SemVer prerelease: breaking changes inside the 3.0 beta line are allowed and announced in the changelog. The `.mq.md` source language changed incompatibly (RFC-LANG-0002); there is no old-syntax execution path or downgrade. CLI flags may change with a changelog note; `strict` refusal is contractual. |
| `marqdo.lock` | v4 | v3 is rejected with an explicit `marqdo lock` migration error; v1/v2 are rejected with a relock instruction; `run`/`call`/`check` never rewrite or fetch the lock. `extensions:` records manifest/artifact identity (integrity, not publisher auth). |
| Analysis | v2/v3 on the RFC-CTRL-0005 development branch | Projects with loop transfers emit v3. Other projects emit v2. Consumers reject unknown majors. |
| Trace | v2 | Binds to the exact Analysis bytes; unknown majors rejected. |
| Execution Events | v1 | Live NDJSON for `run/call --event-stream jsonl`; independent version axis. Values are opt-in and bounded; unknown majors rejected. Not yet part of the tagged beta archive (post-beta addition). |
| CLI Tool | v1 | Manifest schema, SemVer negotiation and whole-bundle digest are stable for v1. |
| Plugin ABI | v1 | Struct layout/version negotiation frozen; `--trust-native-plugin` is required for in-process loading. The static manifest signature is authoritative; `run` reconciles loaded exports/signatures/owners/artifact digests exactly (`ext.plugin_binding_mismatch`); ABI v1 defaults are rejected. |
| Resource Provider manifest | v2 | `schema_version: 2`, `abi: c-abi-1`, `transport: worker`; v1 is rejected (`ext.unsupported_provider_schema`), no silent migration. CLI `--provider`/`--allow-provider`; trusted worker, not an OS sandbox. The six core kinds come from one `marqdo_host::CORE_RESOURCE_KINDS` registry and are rejected as a provider name or kind (ADR-0062). |
| Callable resolution | ADR-0054 | One registry + one binding pass; `check` and `run` bind the same `CallSiteKey`; no runtime name fallback. Import aliases colliding with builtin/plugin dotted namespaces are rejected. The `math` namespace is no longer a builtin namespace (ADR-0061). |
| Analysis `calls[].state/target` | v2 (projection) | Now projected from the unique binding; official stdlib aliases appear as `function`. Schema/version unchanged; private binding table not serialized. |
| Extension selection | lock v4 | `marqdo.yaml.extensions` only `./path` or `@alias/path`; declaration is not a grant; declared identity must match the lock (`ext.extension_lock_mismatch`). |
| Foreign wire | v1 | One-shot request envelope; provider descriptors are explicit. |

## Platform / feature evidence

Legend: **PASS** = executed on this snapshot with logs; **UNVERIFIED** = code is
portable/tested locally but the native platform lane has not run here;
**UNSUPPORTED** = deliberately not claimed; **FAIL-CLOSED** = request is refused.

| Feature | Ubuntu Linux x86_64 | macOS arm64 | Windows x86_64 |
|---|---|---|---|
| Markdown profile / AST goldens (160) | PASS | UNVERIFIED (cfg fix + CRLF fix) | UNVERIFIED (cfg fix + CRLF fix) |
| Rust interpreter / unified semantics (73) | PASS | UNVERIFIED | UNVERIFIED |
| Addressable `Memory` P1 (ADR-0022) | PASS | UNVERIFIED | UNVERIFIED |
| Resource streams CM-A06 (ADR-0025/0026: stdout + project file read/write) | PASS | UNVERIFIED | **FAIL-CLOSED** (`strict` refuses resource opens) |
| `run` / `call` / `check` / `inspect` / `fmt` | PASS | UNVERIFIED | UNVERIFIED |
| lock v4 path/Git graph + sync + tamper | PASS | UNVERIFIED | UNVERIFIED |
| Extension selection/lock v4 (`extensions:`, identities, `inspect --extensions`) | PASS | UNVERIFIED | UNVERIFIED |
| Callable single binding + `ext.import_namespace_conflict`/`symbol.unknown`/plugin reconciliation | PASS | UNVERIFIED | UNVERIFIED |
| Official stdlib public aliases (print de-magic; byte-identical output) | PASS | UNVERIFIED | UNVERIFIED |
| Resource Provider v2 CLI (`--provider`/`--allow-provider`, worker helper) | PASS (linux/x86_64; real C sensor + helper) | **FAIL-CLOSED** (`ext.platform_unsupported`) | **UNSUPPORTED** |
| Capability handler registry (`fs.read` handler, audit, unavailable) | PASS | UNVERIFIED | UNVERIFIED |
| External conformance repository (`marqdo-extension-conformance`, frozen CLI) | PASS (`--release` binary sha256 `28e3776d…`, fixture repo `e4b7352c…`; see `docs/evidence/extension/6a7bb983.../manifest.json`) | UNVERIFIED | UNVERIFIED |
| Analysis v2 / Trace v2 + Python consumers | PASS | UNVERIFIED | UNVERIFIED |
| Execution Events v1 (`--event-stream jsonl`, P0/P1) | PASS locally (14 CLI contract tests + 8 host + 4 runtime; independent Python schema consumer); post-beta, not in the tagged archive | UNVERIFIED | UNVERIFIED |
| Tool Protocol v1 + 3 independent consumers | PASS | UNVERIFIED | UNVERIFIED |
| Native ABI (`plugin inspect`, trusted call) | PASS (C fixture) | UNVERIFIED (host-native fixture) | **UNSUPPORTED** |
| Worker ABI + `fs.read` bridge | PASS (Unix) | UNVERIFIED | **UNSUPPORTED** (Unix-gated) |
| Foreign Python/Node | PASS (system `python3`/`node`) | UNVERIFIED | UNVERIFIED |
| Foreign pinned environment binding | PASS | UNVERIFIED | UNVERIFIED |
| `strict` Linux L1 (pure Marqdo + Math) | PASS, probe-gated, 7/7 VM lane | **FAIL-CLOSED** | **FAIL-CLOSED** |
| `strict` L2 (Worker/Foreign/native) | **UNSUPPORTED** (flags refused) | **UNSUPPORTED** | **UNSUPPORTED** |

The Linux column is the supported beta platform. Lanes marked PASS were
executed on the release commit: the Woodpecker Linux pipeline (fmt, clippy,
workspace tests with `--locked`, Windows cross-target compile check, release
build of both binaries), the release-archive cold-install smoke, the external
frozen-binary conformance repository, and the disposable-VM strict lane. The
source of truth is the Gitea remote `git.cflmy.cn/cflmy/marqdo-cli` with
Woodpecker at `ci.cflmy.cn`; a GitHub mirror is not the authoritative lane for
this beta. macOS/Windows entries are implementation notes, not support claims;
the Windows C ABI/provider paths remain fail-closed.

## Strict L1 scope (ADR-0021)

- Enabled only when the runtime Bubblewrap probe succeeds. Probe failure or a
  probe→launch race exits non-zero with a diagnostic and **never** falls back
  to `project`.
- Inside the sandbox: user/mount/PID/net/UTS/IPC namespaces, no-new-privs,
  empty capabilities, read-only `/project`, private `/proc`/`/dev`/`/tmp`,
  rlimits and wall-clock/output supervision; the child verifies `PID 1`,
  `NoNewPrivs`, `CapEff=0`, a non-initial uid map and no non-loopback network.
- Refused before launch: `--allow-read`, `--allow-write`, `--allow-foreign`,
  `--allow-plugin`, `--plugin`, `--trust-native-plugin`, `--kernel`,
  `--trace-file`.
- Strict L2 (letting Worker/Foreign/native plugin run inside the sandbox) is
  not implemented and must not be inferred from L1 passing.

## Minimum environment

- Rust 1.82+ (verified with 1.100.0).
- Linux/macOS: a C compiler for Native ABI fixtures; Python/Node only for
  Foreign kernels; `git` for Git dependencies.
- `strict` additionally requires Bubblewrap and a kernel/environment that
  permits user, mount, PID and network namespaces. See
  [`docs/security-gate.md`](docs/security-gate.md) and ADR-0021.

## RFC-EXT-0001 platform statements

* Provider worker transport: linux/x86_64 only; macOS/Windows return
  `ext.platform_unsupported` (compile-only, fail-closed). The RFC's macOS
  provider lane is **not claimed**.
* Windows Native/Worker ABI remains **UNSUPPORTED**; a Linux PASS does not
  substitute for it.
* `strict` L1 continues to refuse plugins/providers/foreign kernels; strict L2
  is not implemented. A worker is a trusted process, not an OS sandbox.
* The external conformance suite is a mandatory **additional** release gate
  (`docs/releasing.md` §7); it does not replace G3/G4/native lanes.

## Explicitly unsupported in this beta

- Pre-RFC `.mq.md` source syntax (Strong-execute / Emph-return / ordered-list
  branch / bullet-list loop / block-quote call / first code-span signature).
  Old sources are a rewrite task, not a compatibility mode: there is no
  `--legacy`, `marqdo_semantics` selector or second mapper.
- OS-level strict isolation on macOS/Windows (Linux only, probe-gated).
- Strict L2: Worker/Foreign execution inside strict (refused in L1).
- Windows Native/Worker ABI runtime support.
- Automatic uv/npm/venv provisioning; only an explicitly provisioned, pinned
  directory is verified (and must contain a regular-file executable).
- Registry/package publishing, JIT, Knowledge/Proof/View built-ins,
  sessions, and general-purpose CAS. The three `sample-*` tools are test
  fixtures for the generic Tool protocol, not product features.
- Tagged Windows/macOS release archives: only the Linux x86_64 archive is
  produced for `3.0.0-beta.1`. macOS/Windows native artifacts are not claimed;
  the Windows lane is a Linux-hosted compile gate only.

## Release decision

**`3.0.0-beta.1` prerelease — approved as a Linux x86_64 beta with explicitly
recorded gaps.** The beta profile in
[`docs/MARQDO_3_0_PRERELEASE_PLAN.md`](docs/MARQDO_3_0_PRERELEASE_PLAN.md) §3.1
was used, not the stable profile: Linux is the only supported platform,
macOS/Windows are declared unsupported/experimental, G4 is a Beta-only `WARN`
on the fixed Woodpecker agent, and strict policy and frozen-binary conformance
are recorded with same-SHA evidence in [`docs/STATUS.md`](docs/STATUS.md) and
[`docs/evidence/ci/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`](docs/evidence/ci/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json)
(`BETA_RELEASE_CANDIDATE`). Release:
https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-beta.1. Beta status
does not imply stable `3.0.0` or LTS; the stable gate remains stricter and
separate.
