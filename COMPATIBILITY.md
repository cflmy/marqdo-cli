# Compatibility and support matrix

Snapshot: workspace version **0.1.0**, language SHA **`9c1723d`**, security SHA
**`8af7262`**, no release tag cut. This file records what was actually verified and separates it
from what is only implemented. Executed status is in
[`docs/STATUS.md`](docs/STATUS.md).

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

## Public protocol versions

| Surface | Version | Compatibility rule |
|---|---|---|
| Markdown profile | `marqdo.md.v1` | Frozen parser/options. The RFC-LANG-0002 mapping rewrite keeps this profile; the source-language break is tracked above, not by a new profile id. |
| Language / CLI | workspace `0.1.0` | Pre-1.0. The `.mq.md` source language changed incompatibly (RFC-LANG-0002); there is no old-syntax execution path. CLI flags may change with a changelog note; `strict` refusal is contractual. |
| `marqdo.lock` | v3 | v1/v2 are rejected with a relock instruction; `run`/`call`/`check` never rewrite or fetch the lock. |
| Analysis | v2 | Consumers reject unknown majors; incompatible field changes require v3. |
| Trace | v2 | Binds to the exact Analysis bytes; unknown majors rejected. |
| CLI Tool | v1 | Manifest schema, SemVer negotiation and whole-bundle digest are stable for v1. |
| Plugin ABI | v1 | Struct layout/version negotiation frozen; `--trust-native-plugin` is required for in-process loading. |
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
| lock v3 path/Git graph + sync + tamper | PASS | UNVERIFIED | UNVERIFIED |
| Analysis v2 / Trace v2 + Python consumers | PASS | UNVERIFIED | UNVERIFIED |
| Tool Protocol v1 + 3 independent consumers | PASS | UNVERIFIED | UNVERIFIED |
| Native ABI (`plugin inspect`, trusted call) | PASS (C fixture) | UNVERIFIED (host-native fixture) | **UNSUPPORTED** |
| Worker ABI + `fs.read` bridge | PASS (Unix) | UNVERIFIED | **UNSUPPORTED** (Unix-gated) |
| Foreign Python/Node | PASS (system `python3`/`node`) | UNVERIFIED | UNVERIFIED |
| Foreign pinned environment binding | PASS | UNVERIFIED | UNVERIFIED |
| `strict` Linux L1 (pure Marqdo + Math) | PASS, probe-gated, 7/7 VM lane | **FAIL-CLOSED** | **FAIL-CLOSED** |
| `strict` L2 (Worker/Foreign/native) | **UNSUPPORTED** (flags refused) | **UNSUPPORTED** | **UNSUPPORTED** |

The Ubuntu column is from this development host (Ubuntu 26.04 container) and a
disposable Ubuntu 26.04 QEMU/TCG VM for the security lane. It is **not** a
GitHub Actions result: this checkout has a Gitee remote and no GitHub
credentials. A fresh local clone with `core.autocrlf=true` was used to
reproduce the Windows checkout byte behavior.

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

## Explicitly unsupported in this candidate

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
- Windows/macOS release archives from this host; the GitHub release job may
  build macOS but has not run for the convergence SHA.

## Release decision

**DO NOT RELEASE.** The RFC-CI-0001 series fixed the platform `E0425` build
break and the strict `--json` empty-stdout defect (ADR-0053), Woodpecker is
green at `0d7e11b`, and the G3 lane PASSes locally as a normal user. However
the same SHA has **no remote Windows/macOS native PASS, no remote G3 security
artifact and no `MEASURED` G4 performance report**. Those gates remain
`UNVERIFIED`; a local or cross-target PASS is not a platform/security release
proof. See [`docs/STATUS.md`](docs/STATUS.md) §0.1 and
[`docs/evidence/ci/README.md`](docs/evidence/ci/README.md).
