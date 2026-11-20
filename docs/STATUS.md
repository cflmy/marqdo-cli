# Marqdo Implementation Status

Snapshot: **2026-11-20**, workspace version `0.1.0`, no release tag.
Authoritative CI convergence plan:
[`NEXT_RELEASE_CI_CONVERGENCE.md`](NEXT_RELEASE_CI_CONVERGENCE.md).
Support matrix: [`../COMPATIBILITY.md`](../COMPATIBILITY.md).

This file records what was actually executed on the current tree. "Implemented"
does not mean "verified on every platform"; the evidence column says which
commands were run and where. The historical M0.5 snapshot is preserved at
[`history/STATUS-M0.5-snapshot.md`](history/STATUS-M0.5-snapshot.md) and must
not be read as current status.

## 1. Remote baseline and current evidence

GitHub `master` is still `c9f01aa8fb127a2dcc125103ef794832984774ad`; the latest
public CI is run
[44692692318](https://github.com/cflmy/marqdo-cli/actions/runs/44692692318)
and is **red**:

- Linux `fmt + clippy + test`: FAIL —
  `strict_mode_enforces_isolation_or_refuses_closed` hit
  `bwrap: Creating new namespace failed: Resource temporarily unavailable`.
- macOS / Windows `cargo test --workspace`: FAIL — `E0433` on
  `linux::probe()` under `cfg!`.
- Schema instance validation + consumers: PASS.
- Release candidate artifact: SKIPPED (upstream failure; tag/dispatch only).

This checkout has a Gitee remote and **no GitHub credentials**. No new remote
CI URL can be produced from this host. The evidence below is local/VM evidence
for the convergence commit, not a substitute for the native GitHub lanes.

Convergence code/security SHA: **`f075c967f24d885ac42c1e99329bee3498154598`**
(the following docs commit only adds this report and release material; code is
byte-identical).

### Local static and test gate (Ubuntu 26.04, Rust 1.100.0)

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  (47 test binaries, 0 failed; includes 7/7 Linux security tests)
```

### G-C security lane (disposable Ubuntu 26.04 VM, ordinary user `dev`)

Source: `git archive f075c96` extracted in the VM (not the dev container).
Host: QEMU/TCG, kernel `7.0.0-43-generic`, `systemd-detect-virt --container`
= `none`, bubblewrap 0.11.1 with `bwrap-userns-restrict`, uid 1000.

```text
./tests/security/run-lane.sh
  adversarial helper matrix: 7 passed, 0 failed, 0 ignored
  positive CLI contract: PASS
  external-execution surfaces refused: PASS
  forged active marker rejected: PASS
  G-C security lane: PASS
cargo test --workspace --locked (same archive): 47 ok, 0 failed
```

Logs: [`evidence/2026-11-20-strict-gc-f075c96.md`](evidence/2026-11-20-strict-gc-f075c96.md).
This is still a **local VM lane**, not a self-hosted GitHub Actions security URL.

## 2. Work package status

| # | Work package | State | Evidence |
|---|---|---|---|
| 0 | Windows AST line endings | Fixed locally; **Windows CI UNVERIFIED** | `.gitattributes`, CRLF fixture |
| 1 | macOS native plugin fixtures | Fixed locally; **macOS CI UNVERIFIED** | host-target manifest + dylib flags |
| 2 | Cross-platform strict `cfg` | **Fixed locally; native CI UNVERIFIED** | `#[cfg]` dispatch + non-Linux regression test |
| 3 | Strict probe→launch race | **Fixed locally; GitHub Linux CI UNVERIFIED** | fail-closed CLI contract + probe-OK/launch-EAGAIN seam test |
| 4 | Parallel security matrix isolation | **Fixed** | unique paths + helper `OnceLock`; 7/7 in parallel workspace run |
| 5 | Markdown/power/control-flow corpus | **Done locally** | 160 AST, 44 semantic, 52 compile-negative, 12 runtime-negative fixtures |
| 6 | Condition header + source spans | **Fixed** | executable tails rejected; literal-inside-backtick mapping tests |
| 7 | Independent Tool consumers | **Done locally** | `sample-view`/`sample-knowledge`/`sample-proof` E2E test |
| 8 | lock v3 reproducibility | **Implemented; tamper test added** | `package_env` PASS |
| 9 | Analysis/Trace/Tool/ABI/Foreign | **Implemented; supported-matrix bounded** | protocol tests, plugin/worker/foreign tests |
| 10 | Linux strict L1 | **VM PASS at f075c96; no CI URL** | 7/7 + positive CLI |
| 11 | Linux strict L2 | **NOT IMPLEMENTED / UNSUPPORTED** | strict rejects Worker/Foreign/native ABI |
| 12 | Windows Native/Worker ABI | **UNSUPPORTED** | no Windows ABI artifact/claim |
| 13 | RC artifact | **Linux candidate only; no release** | clean-install + checksums; macOS not built here |
| 14 | Release approval | **NOT STARTED** | requires owner approval |

## 3. Verified capabilities

- Frozen Markdown profile `marqdo.md.v1` with 160 AST goldens and explicit
  CRLF byte semantics.
- Native Rust interpreter with 44 semantic goldens, 52 compile-error fixtures,
  and runtime error source attribution.
- Native `**` power is pinned end-to-end: right associativity, unary-minus
  precedence, parentheses, float negative exponents, branch/loop conditions,
  Strong+CodeSpan payloads, and inert prose/code fences.
- lock v3 full transitive package-instance graph, verified Git cache, strict
  `sync --locked`, atomic `.marqdo/env` materialization; `run`/`check` never
  fetch or rewrite the lock. Locked path content tamper is rejected without
  rewriting the lock or environment.
- Analysis v2 (logical identity, resources, byte digest) and Trace v2 (module
  ownership, bounds, truncation, stale detection) with independent Python
  consumers and JSON Schema validation.
- Tool Protocol v1 with arbitrary command names (no CLI hard-coding),
  SemVer/protocol negotiation, whole-bundle digest verification, atomic
  install/remove, and three independent Analysis/Trace consumers.
- Plugin ABI v1: native + worker transports, capability broker executing
  `fs.read` through `ProjectFs`, per-invocation budgets, host-owned handle
  identity, crash/timeout isolation. Not an OS sandbox.
- Foreign kernels: one-shot Python/Node, typed wire, timeout/output caps,
  provider/environment digests, environment-bound provider, interpreter
  version verification.
- Linux strict L1: Bubblewrap namespace supervisor, runtime probe, fail-closed
  launch, child self-verification, 7/7 adversarial matrix.

## 4. Open blockers and limits

1. **No remote CI evidence for the convergence SHA.** Windows/macOS fixes and
   the Linux EAGAIN contract are UNVERIFIED on GitHub. G1/G2/G3 remain FAIL
   until a real run exists.
2. **G-C is VM-verified but not CI-signed.** No self-hosted security-lane URL
   for f075c96 exists.
3. Windows Native/Worker ABI is explicitly UNSUPPORTED (no Windows `cc`
   fixture lane, Unix-gated worker tests).
4. `fs.write`, `net.request`, `process`, `clock`, `random` are not executed
   broker services; unsupported requests must return `capability.unavailable`.
5. uv/npm/venv provisioning is not implemented. Only a pre-provisioned,
   digest-pinned directory with a regular-file executable is supported.
6. No Windows/macOS RC archive exists from this host. The release-candidate
   workflow can build macOS on GitHub, but it has not run for this SHA.
7. No release tag/binary publication. The release decision is **DO NOT
   RELEASE** until the native CI matrix and a security-lane CI URL exist.

## 5. Evidence files

- [`evidence/2026-11-20-local-gate.md`](evidence/2026-11-20-local-gate.md) —
  earlier local gate and Windows autocrlf reproduction.
- [`evidence/2026-11-20-strict-gc-f075c96.md`](evidence/2026-11-20-strict-gc-f075c96.md)
  — current VM lane, environment, full log and workspace test log.
- [`evidence/RELEASE_CANDIDATE_0.1.0_f075c96.md`](evidence/RELEASE_CANDIDATE_0.1.0_f075c96.md)
  — G0..G12 matrix and RC decision.
- [`security-gate.md`](security-gate.md) — mechanism, attack matrix and strict
  L1/L2 limits.

## 6. Reproduce

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# On hosts with a read-only $HOME, point CARGO_HOME at a writable directory.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli
```

Consumer evidence:

```bash
./target/debug/marqdo check examples/multi-module/index.mq.md --refs --emit /tmp/a.json
./target/debug/marqdo run examples/multi-module/index.mq.md --trace-file /tmp/t.json
python3 tests/consumers/analysis_consumer.py /tmp/a.json
python3 tests/consumers/trace_consumer.py /tmp/a.json /tmp/t.json
```

G-C (ordinary user, disposable VM only):

```bash
MARQDO_REQUIRE_STRICT_SECURITY=1 ./tests/security/run-lane.sh
```
