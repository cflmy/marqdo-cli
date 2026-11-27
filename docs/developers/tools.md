# External tools: current CLI Tool protocol

The core does not contain `view`, `knowledge`, `prove`, `lint` or any other
product command. A tool is a **separate executable** registered by the user; the
CLI only dispatches unknown top-level commands to registered tools and never
scans `PATH` for a guessed name.

Normative contract: [`../../spec/cli-tool.md`](../../spec/cli-tool.md) and
[`../../spec/schemas/cli-tool.schema.json`](../../spec/schemas/cli-tool.schema.json).

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

`tool.json` for the `3.0.0-rc.6` candidate:

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.1.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "requires": {"api_revision": 1, "analysis": 3, "observation": 1}
}
```

- `version` is the **tool's** SemVer. It identifies the artifact; it does not
  promise compatibility with retired core protocols.
- `requires.api_revision` is required and must equal the current core revision.
- `requires.analysis` and `requires.observation` are optional. Declare one only
  when the tool consumes that contract, and declare it as a **single exact
  integer**; arrays, ranges and "highest shared version" negotiation do not
  exist.
- A mismatched requirement fails install and dispatch with
  `tool.protocol_incompatible`; there is no negotiation and no fallback.
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
| `MARQDO_API_REVISION` | the single current tool API revision (`1`) |

Call `$MARQDO_BIN` with an argv array; do not assume `marqdo` is on `PATH`.
The retired `MARQDO_TOOL_PROTOCOL`, `MARQDO_PROTOCOL_ANALYSIS` and
`MARQDO_PROTOCOL_TRACE` variables are not exported: version selection is not
negotiated. A registration written by a retired manifest/record format is
rejected with a "remove and reinstall" diagnostic and is never rewritten.

## 3. Consume the public protocols

Static input (View/Knowledge/Proof):

```bash
"$MARQDO_BIN" check index.mq.md --refs --emit ./analysis.json
```

Dynamic input uses the single Observation contract. The same run can deliver
the event stream on stdout and an atomic JSON snapshot in one invocation:

```bash
# live JSONL events
"$MARQDO_BIN" run index.mq.md --output jsonl
"$MARQDO_BIN" call index.mq.md main --arg n=5 --output jsonl

# one JSON snapshot, from the same execution
"$MARQDO_BIN" run index.mq.md --output json --observation-file ./observation.json

# both destinations, still one execution
"$MARQDO_BIN" call index.mq.md main --arg n=5 --output jsonl \
  --observation-file ./observation.json
```

Consume the stdout line stream as NDJSON; do not pass another `--output` in the
same invocation. Every event carries `schema: "marqdo.observation"` and
`schema_version: 1`; read `stream.opened` for the capture policy and reject any
other format identifier. The JSON snapshot wraps the same events, so a consumer
must not treat it as a second protocol or renumber them. A tool server may
convert the stream to its own SSE/websocket transport; the CLI never implements
HTTP/SSE. Live and file destinations are rejected under `--mode strict` in the
current tree; the tool must surface that refusal instead of pretending
real-time support. The retired `--json`, `--trace-file` and `--event-stream`
options are rejected, not aliased.

See [analysis-observation.md](analysis-observation.md) for fields and digest rules. A
consumer must verify the exact Analysis bytes (SHA-256) and each module source
digest before attributing a span. Do not depend on `inspect --ir`: that JSON is
not a frozen cross-repository protocol.
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
- Cover the negative paths: unknown schema/schema_version, incompatible
  `api_revision`/`analysis`/`observation` requirement, array requirements,
  retired record format, reserved command, traversal/symlink entrypoint,
  tampered bundle file, missing entrypoint, tool crash/exit-status propagation.
- Validate against the current Analysis and Observation schemas
  (`spec/schemas/`) and check `source.analysis_digest` plus the module source
  digest before using event spans.
- Include README, minimal example, supported CLI product range, the exact
  protocol requirements, platform matrix, reproducible artifact hashes and
  limitations.
- The archive ships `examples/tool-samples/{sample-view,sample-knowledge,sample-proof}`
  as independent source examples and `examples/tool-unknown-name/` as a
  runnable dispatch fixture.
