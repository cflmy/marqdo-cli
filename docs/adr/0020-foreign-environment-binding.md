# ADR-0020: A pinned Foreign environment binds the executed provider

Status: **Accepted**
Date: 2026-11-20
Relates to: ADR-0012, `spec/foreign.md` §2/§6, stable-release spec §8.3

## Context

`KernelDescriptor` can declare `environment_id`, `environment_digest`,
`environment_dir` and `interpreter_version`. Before this ADR the host verified
the environment tree digest and then prepended `<env>/bin` to `PATH`, but the
executed program was still resolved from the host `PATH` for built-in providers,
and external entrypoints could live outside the verified environment. A
manifest could therefore pass integrity verification while a different
interpreter (or a host interpreter whose dependencies were not part of the
pinned tree) actually ran.

## Decision

1. `environment_dir` is an execution binding:
   * an external provider's absolute `entrypoint` must resolve inside the
     canonicalized environment; otherwise registration/execution fails with
     `ForeignError::Protocol`;
   * one of the built-in providers cannot be combined with
     `environment_dir`; a pinned interpreter is registered as an explicit
     external provider whose entrypoint is the interpreter itself.
2. A bound child receives `MARQDO_FOREIGN_ENV` and exactly the environment's
   executable directory on `PATH`. The host `PATH` is not appended. On Windows
   `SYSTEMROOT`/`WINDIR` are inherited for process creation, not for package
   resolution.
3. `interpreter_version`, when declared, is checked against the resolved
   program's `--version` output before the program source is executed. A
   mismatch fails closed.
4. The profile-v1 environment digest keeps rejecting symlinks and special
   files. A virtualenv whose interpreter is a symlink is not pin-able in this
   release; this limitation is documented rather than silently resolved.
5. This does not make the child an OS sandbox. Host authorization and
   `spec/security.md` still govern capabilities; strict mode remains refused
   until its own ADR and security lane pass.

## Consequences

* A verified environment can no longer be decorative: either the program comes
  from it, or the descriptor is rejected.
* Existing external descriptors that declared `environment_dir` while pointing
  at a host binary must be corrected; this is an intentional fail-closed
  behavior change, documented in the changelog.
* uv/npm environment *creation* remains out of scope; an external provisioner
  may produce a directory that satisfies this contract.
* Built-in Python/Node without an environment keep their PATH lookup and
  install hint.

## Tests

`crates/marqdo-foreign/tests/provider.rs`:

* `pinned_environment_binds_the_provider_and_is_verified_before_execution`
* `provider_outside_pinned_environment_is_rejected`
* `builtin_provider_cannot_be_bound_to_an_environment`
* `declared_interpreter_version_must_match_the_resolved_program`
* existing digest/tamper/timeout/registration tests remain green.
