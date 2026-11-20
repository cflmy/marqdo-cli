# Marqdo Implementation Status

Snapshot: **2026-11-20**, workspace version `0.1.0`, no release tag.
Authoritative task plan: [`STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md).
Support matrix: [`../COMPATIBILITY.md`](../COMPATIBILITY.md).

This file records what was actually executed on the current tree. "Implemented"
does not mean "verified on every platform"; the evidence column says which
commands were run and where. The historical M0.5 snapshot is preserved at
[`history/STATUS-M0.5-snapshot.md`](history/STATUS-M0.5-snapshot.md) and must
not be read as current status.

## 1. CI baseline and current evidence

The last known public GitHub CI at baseline `e966b76` (run
[44680378977](https://github.com/cflmy/marqdo-cli/actions/runs/44680378977))
was **not green**:

- Linux `fmt + clippy + workspace tests + smoke`: PASS.
- Schema instance validation + Python consumers: PASS.
- macOS `cargo test --workspace`: FAIL —
  `plugin_inspect_and_explicit_trusted_invocation` used a Linux `.so` and a
  Linux-target manifest.
- Windows `cargo test --workspace`: FAIL — AST golden bytes were CRLF after
  checkout while the parser emits LF, producing "0 line(s) differ".

This checkout has a Gitee remote only and no GitHub Actions credentials. **No
new remote CI URL can be produced from this host.** The local gate below is
real evidence for this machine, not a substitute for the native Windows/macOS
lanes.

### Local gate after the stabilization commits

All commands were run from the repository root with Rust 1.100.0:

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                PASS
cargo test -p marqdo-cli --test conformance_ast       PASS (4 tests)
cargo test -p marqdo-cli --test integration_cli       PASS (23 tests)
cargo test -p marqdo-cli --test plugin_worker         PASS (4 tests)
cargo test -p marqdo-foreign --test provider          PASS (8 tests)
```

Windows checkout reproduction: a fresh local clone with
`git -c core.autocrlf=true clone ...` kept goldens at LF, preserved the CRLF
byte fixture, and passed all four `conformance_ast` tests. The previous
behavior was reproduced by converting one golden to CRLF: the new diagnostic
reports the first differing byte and "the only difference is line endings".

macOS reproduction is **not possible on this host**; the fix removes the Linux
assumption by generating the manifest from the host triple and building
`solver.dylib` with `-dynamiclib`. It remains UNVERIFIED until the macOS lane
runs.

## 2. Work package status

| # | Work package | State | Evidence |
|---|---|---|---|
| 0 | Windows AST line endings | **Fixed locally, Windows CI UNVERIFIED** | `.gitattributes`, byte diagnostics, CRLF structure test, autocrlf clone PASS |
| 1 | macOS native plugin fixtures | **Fixed locally, macOS CI UNVERIFIED** | host-target manifest + dylib flags, foreign-target/tamper/ABI tests PASS |
| 2 | Status/docs reconcile | **Done** | this file, `COMPATIBILITY.md`, `CHANGELOG.md`, `docs/releasing.md`, `docs/security-gate.md` |
| 3 | Package lock v3 reproducibility | **Implemented; targeted audit pending** | ADR-0016, `cargo test -p marqdo-cli --test package_env` PASS |
| 4 | Analysis/Trace/Tool protocol | **Implemented; consumers PASS locally** | schemas + `tests/consumers/*.py`, `analysis_trace`/`tool_protocol` PASS; `check_never_spawns_a_foreign_kernel` PASS |
| 5 | Foreign pinned environment | **Done (ADR-0020)** | provider tests PASS; env binds interpreter; version drift rejected |
| 6 | ABI session/broker hardening | **Partial** | `fs.read` bridge + budgets + handle tests PASS; Windows ABI unsupported; broader services still `capability.unavailable` |
| 7 | Linux strict probe | **Done as fail-closed evidence** | `docs/security-gate.md`; userns/bwrap EPERM, cgroup ro, sudo blocked |
| 8 | Linux strict pure (L1) | **BLOCKED** | no namespace creation on this host; no adapter implemented or verified |
| 9 | Linux strict extensions (L2) | **BLOCKED** | depends on L1 |
| 10 | Native platform matrix | **Partial** | Linux PASS; macOS/Windows source fixes, remote lanes absent |
| 11 | Independent ecosystem consumers | **Partial** | Analysis/Trace Python consumers PASS; three Tool fixtures not added |
| 12 | RC/docs/artifacts | **Candidate build only** | release docs, checksummed artifact, clean-install smoke; NOT released |

## 3. Verified capabilities

- Frozen Markdown profile `marqdo.md.v1` with 150 AST goldens and explicit
  CRLF byte semantics.
- Native Rust interpreter with 36 semantic goldens, 48 compile-error fixtures,
  and runtime error source attribution.
- lock v3 full transitive package-instance graph, verified Git cache, strict
  `sync --locked`, atomic `.marqdo/env` materialization; `run`/`check` never
  fetch or rewrite the lock.
- Analysis v2 (logical identity, resources, byte digest) and Trace v2 (module
  ownership, bounds, truncation, stale detection) with independent Python
  consumers and JSON Schema validation.
- Tool Protocol v1 with arbitrary command names, SemVer/protocol negotiation,
  whole-bundle digest verification, atomic install/remove.
- Plugin ABI v1: native + worker transports, capability broker executing
  `fs.read` through `ProjectFs`, per-invocation budgets, host-owned handle
  identity, crash/timeout isolation. Not an OS sandbox.
- Foreign kernels: one-shot Python/Node, typed wire, timeout/output caps,
  provider and environment digests, and (ADR-0020) environment-bound provider
  with interpreter-version verification.

## 4. Open blockers and limits

1. **No remote CI evidence for this commit.** The repository remote is Gitee;
   no GitHub workflow was executed. Windows/macOS fixes are UNVERIFIED there.
2. **G-C is not passed.** `strict` remains `Unavailable`; the host cannot
   create namespaces and root cgroup2 is read-only. See
   [`security-gate.md`](security-gate.md). No release may claim Linux strict
   isolation.
3. Windows Native/Worker ABI is explicitly unsupported (no `cc` toolchain in
   the lane); it is not counted as a pass.
4. `fs.write`, `net.request`, `process`, `clock`, `random` are not executed
   broker services; unsupported requests must return `capability.unavailable`.
5. uv/npm/venv provisioning is not implemented. Only a pre-provisioned,
   digest-pinned directory with a regular-file executable is supported.
6. Three independent Tool consumers (`sample-view`/`knowledge`/`proof`) are
   not yet added; only the portable `benchmark` fixture and Analysis/Trace
   Python consumers exist.
7. No release tag/binary publication. A candidate archive and checksums are
   produced locally; the release decision is **DO NOT RELEASE** until the
   native CI matrix and G-C conditions in the plan are satisfied.

## 5. Reproduce

```bash
export PATH="$HOME/.cargo/bin:$PATH"
# On hosts with a read-only $HOME, point CARGO_HOME at a writable directory.
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --locked -p marqdo-cli
```

Consumer evidence:

```bash
./target/debug/marqdo check examples/multi-module/index.mq.md --refs --emit /tmp/a.json
./target/debug/marqdo run examples/multi-module/index.mq.md --trace-file /tmp/t.json
python3 tests/consumers/analysis_consumer.py /tmp/a.json
python3 tests/consumers/trace_consumer.py /tmp/a.json /tmp/t.json
```
