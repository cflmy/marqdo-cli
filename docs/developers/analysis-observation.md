# Analysis and Observation

Run the same public CLI that users install.

```sh
marqdo check index.mq.md --emit ./analysis.json
marqdo call index.mq.md main --output jsonl --observation-file ./run.json
```

A tool can use [Analysis](../../spec/analysis.md) to locate functions, imports,
references, resources and calls. Validate its identifier before walking the
module and statement arrays. Use the original UTF-8 bytes for offsets and
digest checks. The format is the same for a simple document and one using
structured loop transfers.

[Observation](../../spec/observation.md) carries frames, control flow, committed
output and the final result. Consume JSONL for live updates, or use
`--output json` for one JSON snapshot on stdout. The snapshot's `events`
array has the same objects as the live stream. The first event opens the run
and the terminal event gives its outcome. Treat a stream that ends before a
terminal as incomplete. Base64 decode each stdout and stderr chunk and verify
its byte count. Read the typed return from `run.completed.payload.value`.

Before highlighting source locations, compare the `run.started` root and
SHA-256 of the exact Analysis artifact, then check the source file digest.
A different report or edited source requires a fresh check and run.
The repository's independent consumer implements both delivery modes.

```sh
python3 tests/consumers/observation_consumer.py ./run.json --snapshot \
  --analysis ./analysis.json --source-root .
```

Value capture uses exact names and explicit Text consent. Use
`--observe-variable NAME` and `--observe-parameter NAME` only for the
values the feature needs. Request `--observe-text-values` when the caller
has opted into this disclosure. Program output and explicit returned values
can themselves contain sensitive data.

Update integrations that previously used the retired run/call JSON,
Trace, Execution Events or retired run/call options. Use the commands above,
remove the old
reader, and rebuild and reinstall the Tool with the
[current manifest](../../spec/cli-tool.md).
