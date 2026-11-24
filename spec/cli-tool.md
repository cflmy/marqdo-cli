# Marqdo CLI Tool

A CLI Tool is an independent executable registered as a Marqdo subcommand.
It uses public CLI commands and runs with the invoking user's OS permissions.
Plugin trust, Provider grants, Foreign grants and the language sandbox apply
only at their own boundaries.

## Manifest

Put `tool.json` alongside the package's executable and support files.

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "demo.viewer",
  "version": "0.3.0",
  "command": "demo-view",
  "entrypoint": "bin/demo-view",
  "requires": {"api_revision": 1, "analysis": 3, "observation": 1}
}
```

The [manifest schema](schemas/cli-tool.schema.json) is authoritative.
`version` identifies the tool package and uses SemVer. `api_revision` is
required. Declare `analysis` and `observation` only when the tool consumes
those contracts. Every declared requirement is a single exact integer.

## Install and run

Use `marqdo tool install PATH`, `marqdo tool list` and
`marqdo tool remove COMMAND`. Run this sample as `marqdo demo-view ...`.
Commands must match `^[a-z][a-z0-9-]*$` and be available. The entrypoint
must be an executable regular file within the bundle. Symlinks, special
files and root `record.json` are rejected. The CLI verifies and installs
the entire bundle atomically, then checks the installed file set and
SHA-256 digests before execution.

The process inherits standard streams and receives the original arguments,
`MARQDO_BIN` and `MARQDO_API_REVISION`. Tool discovery is explicit through
the install directory, which may be set with `MARQDO_TOOL_HOME`.
To replace a retired registration, remove its command, rebuild the tool
with this manifest and install the rebuilt bundle.
