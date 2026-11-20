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

- `strict` remains `Unavailable` on every platform. This host cannot create
  user/network namespaces (`EPERM` inside a container, cgroup2 read-only, no
  non-interactive sudo), so no Linux OS-isolation adapter was implemented or
  verified. There is no silent downgrade to `project` mode.
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
