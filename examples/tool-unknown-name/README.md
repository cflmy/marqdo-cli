# Unknown-name CLI tool example

This directory is a complete CLI Tool (revision 1) package. Nothing named
`benchmark` exists in the Marqdo CLI source; after installation the unknown
top-level command is dispatched by the registry.

```sh
marqdo tool install examples/tool-unknown-name/tool.json
marqdo tool list
marqdo benchmark run . --some-tool-option
marqdo tool remove benchmark
```

The executable receives the remaining arguments verbatim, inherits stdio/cwd,
and its exit code is propagated. `MARQDO_API_REVISION=1` and `MARQDO_BIN` are
provided for tools that need `check --emit`/`run --output json`.

A Tool is a user-run native process. It is **not** a C ABI plugin, a foreign
kernel, or a program inside the `.mq.md` sandbox.
