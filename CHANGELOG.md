# Changelog

All notable changes to the Marqdo CLI workspace. The workspace version is
currently `0.1.0`; no release tag has been cut. Dates use ISO 8601.

## Unreleased (stabilization candidate)

### Added

- `.gitattributes` byte-identity policy: canonical LF for text files and one
  explicit CRLF byte fixture for Markdown line-ending semantics.
- Byte-level AST golden diagnostics: first differing byte, byte windows and an
  explicit "only line endings differ" classification.
- Cross-platform Native ABI test fixtures generated for the host target, plus
  `no_matching_artifact`, artifact-tamper and ABI-major-mismatch negative tests.
- Host-specific example plugin builds under `examples/plugin-*/build/`; the
  tracked manifests are never rewritten by tests or CI.
- Foreign pinned-environment binding (ADR-0020): an external provider's
  `entrypoint` must resolve inside `environment_dir`, built-in providers cannot
  be bound to one, the child gets only the environment executable directory on
  `PATH`, and `interpreter_version` is verified before execution.
- Linux strict isolation (ADR-0021): a Bubblewrap supervisor with user/mount/
  PID/net/UTS/IPC namespaces, no-new-privs, capability dropping, a read-only
  `/project`, rlimits, output/wall-clock supervision and child
  self-verification. Probe-gated; L1 supports pure Marqdo + Math only.
- Adversarial G-C lane (`tests/security/escape_helper.c`,
  `crates/marqdo-sandbox/tests/security_linux.rs`, `tests/security/run-lane.sh`)
  covering filesystem, `/proc`, network, process, privilege and resource
  escapes. 7/7 PASS in a disposable Ubuntu 26.04 VM.
- Release documentation (`COMPATIBILITY.md`, `docs/releasing.md`) and the
  Linux security-gate evidence report (`docs/security-gate.md`).
- `check_never_spawns_a_foreign_kernel`: proves static `check` does not spawn a
  declared Foreign provider, while `run` on the same document does.

### Changed

- `docs/STABLE_RELEASE_IMPLEMENTATION.md` is the single next-phase
  implementation plan. Older plan documents are historical input, not current
  status.
- AST conformance tests fail early when a golden contains CR bytes instead of
  reporting an opaque mismatch.
- Worker plugin fixtures use the platform dynamic-library name and compiler
  flags.

### Fixed

- Windows CI AST golden failure caused by CRLF checkout of canonical JSON
  goldens (reproduced with a fresh `core.autocrlf=true` clone).
- macOS CI plugin failure caused by assuming a Linux `.so` and Linux-only
  manifest target.

### Security

- Linux `strict` is implemented and probe-gated. It is `Available` only when a
  real Bubblewrap smoke test succeeds; otherwise it stays `Unavailable` and
  refuses. There is no silent downgrade to `project`.
- Strict L1 covers pure Marqdo and native Math; Foreign, native/worker
  plugins, kernels, external grants and trace files are rejected before the
  sandbox starts. Strict L2 inheritance is future work.
- Capability brokers and worker processes remain cooperative
  authorization/crash isolation, not an OS sandbox.

## Baseline `0.1.0` (before this stabilization slice)

- Markdown profile `marqdo.md.v1` with 150 AST goldens.
- Native Rust interpreter, analysis and execution conformance corpus.
- lock v3 package-instance graph with full transitive closure, verified Git
  cache and atomic `.marqdo/env` materialization (ADR-0016).
- Analysis v2 and Trace v2 with independent Python consumers and JSON Schemas.
- Tool Protocol v1 with SemVer/protocol negotiation and whole-bundle digests.
- Plugin ABI v1 native and worker transports with a brokered `fs.read`
  capability path (ADR-0018).
- Pinned Foreign provider/environment digests and one-shot Python/Node kernels.
