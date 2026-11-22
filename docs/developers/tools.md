# External tools: CLI Tool Protocol v1

The core does not contain `view`, `knowledge`, `prove`, `lint` or any other
product command. A tool is a **separate executable** registered by the user; the
CLI only dispatches unknown top-level commands to registered tools and never
scans `PATH` for a guessed name.

Normative contract: [`../../spec/tool-v1.md`](../../spec/tool-v1.md)
(implemented and frozen for protocol v1).

## 1. Package layout

```text
marqdo-view/
  README.md
  tool.json
  bin/marqdo-view            # built artifact; not a core-source dependency
  src/
  tests/contract/
  examples/
```

`tool.json` for the `3.0.0-rc.1` candidate:

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.1.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "compatibility": {
    "marqdo": ">=3.0.0-beta.1,<4.0.0",
    "analysis": [2],
    "trace": [2]
  }
}
```

- `version` is the **tool's** SemVer.
- `compatibility.marqdo` is the **core product** range. A stable-only
  `>=3.0.0,<4.0.0` does not match a beta core; opt in explicitly. The CLI
  rejects a mismatched range at install and dispatch time with a
  `requires marqdo …` diagnostic.
- `analysis`/`trace` are declared only if the tool consumes them; the current
  core provides Analysis v2 and Trace v2.
- `command` must match `^[a-z][a-z0-9-]*$` and must not collide with a built-in
  command (`init`, `run`, `call`, `check`, `inspect`, `fmt`, `plugin`, `add`,
  `remove`, `lock`, `sync`, `tool`, `help`).
- `entrypoint` is package-relative, must be a regular executable file, and the
  whole bundle is hashed (per-file SHA-256 plus a bundle digest) at install and
  re-verified before every dispatch.

## 2. Install, list, dispatch

```bash
marqdo tool install ./marqdo-view        # directory containing tool.json
marqdo tool list
marqdo view --help                       # dispatched to your executable
marqdo view index.mq.md
marqdo tool remove view
```

The CLI preserves `cwd`, stdio, exit status and ordinary arguments (no shell
string interpolation). It does not interpret the tool's own subcommands or
`--help`.

The tool receives:

| Variable | Meaning |
|---|---|
| `MARQDO_BIN` | absolute path of the running `marqdo` executable |
| `MARQDO_TOOL_PROTOCOL` | `1` |
| `MARQDO_PROTOCOL_ANALYSIS` | `2` when Analysis v2 was negotiated |
| `MARQDO_PROTOCOL_TRACE` | `2` when Trace v2 was negotiated |

Call `$MARQDO_BIN` with an argv array; do not assume `marqdo` is on `PATH`.

## 3. Consume the public protocols

Static input (View/Knowledge/Proof):

```bash
"$MARQDO_BIN" check index.mq.md --refs --emit ./analysis.json
```

Dynamic input:

```bash
"$MARQDO_BIN" run index.mq.md --trace-file ./trace.json
```

Live dynamic input (Execution Events v1):

```bash
"$MARQDO_BIN" run index.mq.md --event-stream jsonl
"$MARQDO_BIN" call index.mq.md main --arg n=5 --event-stream jsonl
```

Consume the stdout line stream as NDJSON; do not pass `--json` in the same
invocation. Negotiate by reading `stream.opened` (`marqdo.execution-events`,
`schema_version: 1`) and reject unknown majors. A tool server may convert the
stream to its own SSE/websocket transport; the CLI never implements HTTP/SSE.
This option is unavailable under `--mode strict` in the current tree and the
tool must surface that refusal instead of pretending real-time support.

See [analysis-trace.md](analysis-trace.md) for fields and digest rules. Do not
depend on `inspect --ir`: that JSON is not a frozen cross-repository protocol.
The `view` tool renders the **original Markdown**, not the Analysis JSON; the
Analysis is an index, not a replacement for the source text.

## 4. Security contract

A Tool is a normal process started by the user. It does **not** inherit the
`pure`/`project`/`strict` guarantees of the `.mq.md` programs it reads, and it
must never execute document content implicitly or inherit grants from it.
Checksums verify bundle integrity only; they are not publisher authentication.

## 5. Testing and shipping

- Test against a **frozen** `marqdo` binary: hash it before/after the suite and
  fail if it changes.
- Cover the negative paths: unknown schema major, incompatible `marqdo` range,
  reserved command, traversal/symlink entrypoint, tampered bundle file, missing
  entrypoint, tool crash/exit-status propagation.
- Match a real Analysis v2/Trace v2 schema (`spec/schemas/`) and check
  `source.analysis_digest`.
- Include README, minimal example, supported CLI product range, protocol
  versions, platform matrix, reproducible artifact hashes and limitations.
- The archive ships `examples/tool-samples/{sample-view,sample-knowledge,sample-proof}`
  as independent source examples and `examples/tool-unknown-name/` as a
  runnable dispatch fixture.
