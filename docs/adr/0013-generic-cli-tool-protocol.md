# ADR-0013: Generic external CLI Tool Protocol v1

> Historical. [ADR-0068](0068-single-current-public-contracts.md) supersedes
> the public contracts described in this record.

Status: **Accepted**
Date: 2026-11-20

## Context

M0.6 must let external tools (`view`, `knowledge`, `prove`, or arbitrary
third-party programs) become `marqdo <command>` entries without changing the
core source or command enum. The CLI currently has a closed `clap` enum and no
generic distribution path. C ABI plugins and foreign kernels are unrelated
trust models: a Tool is a user-invoked native process.

## Decision

`spec/tool-v1.md` is implemented as follows:

* new top-level commands are limited to
  `tool install/list/remove` plus fallback for unknown top-level commands;
* `tool.json` (schema `marqdo.cli-tool`, schema_version 1) declares
  `name`, `version`, `command`, `entrypoint` and optional compatibility; the
  core does not contain any tool name, download URL or business argument;
* install requires a local manifest, validates the command (lowercase ASCII,
  no built-in conflict), rejects absolute/`..`/symlink-escaping entrypoints,
  copies the package without following symlinks into a user-controlled tool
  home, re-verifies the executable SHA-256, writes a registry record, and
  publishes atomically via rename;
* dispatch checks the registry record, re-verifies the executable hash, starts
  the entrypoint directly with `OsString` arguments (no shell), inherits
  stdin/stdout/stderr and cwd, sets `MARQDO_BIN`/`MARQDO_TOOL_PROTOCOL=1`, and
  propagates the exit code;
* a missing registration is an unknown command; a `marqdo-<name>` on PATH is
  never executed implicitly;
* uninstall removes only the exact controlled installation directory.

The tool home defaults to the platform config directory and can be redirected
with `MARQDO_TOOL_HOME` for tests/enterprise deployment.

## Alternatives

1. Add a `clap` variant per known tool. Rejected by the audit (E01/E12).
2. Auto-discover `marqdo-*` from PATH (Cargo-style). Rejected: implicit
   execution and PATH hijacking; M0.6 requires explicit installation.
3. Use the C ABI plugin registry. Rejected: different lifecycle, protocol and
   security boundary.
4. Implement a remote registry first. Rejected: local manifest installation is
   the required closed loop; name-based installs are a future optional index.

## Compatibility

This adds one new top-level command (`tool`) and an unknown-command fallback.
Built-in commands always win. Future core commands that collide with an
installed tool are a breaking change and need an ADR/migration policy. The
manifest is versioned; unknown optional fields are ignored in v1 but never
imply authorization.

## Security

A Tool is a normal process run as the user, not inside the `.mq.md` sandbox.
It does not automatically receive Marqdo capabilities, secrets or plugin
handles. Install never runs the tool. The executable hash detects post-install
tampering, not a malicious publisher; local source selection is the trust
decision. Symlinks are rejected during package copy.

## Testing

`crates/cli/tests/tool_protocol.rs` covers arbitrary command names,
install/list/remove, argument/exit-code forwarding, Unicode args, built-in
conflict, traversal, version mismatch, binary tampering and PATH hijacking.
`spec/tool-v1.md` lists the full black-box matrix.

## Migration & rollout

R3 adds the command surface. R4 can expose Analysis/Trace compatibility through
the manifest's optional fields; no tool business logic enters the core.
