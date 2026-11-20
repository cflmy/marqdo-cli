# Compatibility and support matrix

Snapshot: workspace version **0.1.0**, no release tag cut. This file records
what was actually verified and separates it from what is only implemented.

## Public protocol versions

| Surface | Version | Compatibility rule |
|---|---|---|
| Markdown profile | `marqdo.md.v1` | Frozen parser/options; semantic change needs an ADR, new goldens and a profile id bump. |
| Language / CLI | workspace `0.1.0` | Pre-1.0. CLI flags may change with a changelog note; `strict` refusal is contractual. |
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
| Markdown profile / AST goldens | PASS | UNVERIFIED (CRLF fix) | UNVERIFIED (CRLF fix) |
| Rust interpreter / semantics | PASS | UNVERIFIED | UNVERIFIED |
| `run` / `call` / `check` / `inspect` / `fmt` | PASS | UNVERIFIED | UNVERIFIED |
| lock v3 path/Git graph + sync | PASS | UNVERIFIED | UNVERIFIED |
| Analysis v2 / Trace v2 + consumers | PASS | UNVERIFIED | UNVERIFIED |
| Tool Protocol v1 | PASS | UNVERIFIED | UNVERIFIED |
| Native ABI (`plugin inspect`, trusted call) | PASS (C fixture) | UNVERIFIED (host-native fixture) | UNSUPPORTED in CI lane (no `cc`) |
| Worker ABI + `fs.read` bridge | PASS (Unix) | UNVERIFIED | UNSUPPORTED (tests are Unix-gated) |
| Foreign Python/Node | PASS (system `python3`/`node`) | UNVERIFIED | UNVERIFIED |
| Foreign pinned environment binding | PASS | UNVERIFIED | UNVERIFIED |
| `strict` OS isolation | **FAIL-CLOSED / G-C NOT PASSED** | FAIL-CLOSED | FAIL-CLOSED |

The Ubuntu column is from this development host (Ubuntu 26.04 container). It is
**not** a GitHub Actions result: this checkout has no GitHub remote or CI
credentials. A fresh local clone with `core.autocrlf=true` was used to
reproduce the Windows checkout byte behavior.

## Minimum environment

- Rust 1.82+ (verified with 1.100.0).
- Linux/macOS: a C compiler for Native ABI fixtures; Python/Node only for
  Foreign kernels.
- `strict` additionally requires a kernel/environment that permits user,
  mount, PID and network namespaces, plus delegated cgroups. The current
  container does not, so strict is refused. See
  [`docs/security-gate.md`](docs/security-gate.md).

## Explicitly unsupported in this candidate

- OS-level strict isolation on any platform (G-C not passed).
- Windows Native/Worker ABI runtime support.
- Automatic uv/npm/venv provisioning; only an explicitly provisioned, pinned
  directory is verified (and must contain a regular-file executable).
- Registry/package publishing, JIT, Knowledge/Proof/View built-ins,
  sessions, and general-purpose CAS.
