# Marqdo Analysis v3

Analysis v3 extends the public [Analysis v2](analysis-v2.md) contract with
structured loop control. Its envelope keeps `schema: "marqdo.analysis"` and
sets `schema_version: 3`. The writer emits v3 if any emitted function body
contains either new statement, including inside a nested body or an imported
module. Other projects still emit v2. Multi-arm branches fit both versions.

## Statement additions

```json
{"kind": "break", "span": {"start": 30, "end": 37}}
{"kind": "continue", "span": {"start": 39, "end": 49}}
```

Both spans are half-open UTF-8 byte ranges covering the carrier in the
containing module. Walk the enclosing statement bodies to find the nearest
loop. `break` transfers to the statement after that loop. `continue`
transfers to its condition. There are no operands, labels or call edges.
Call and resource projections retain their v2 shapes.

Consumers check the major version before traversing any function body. A v2
reader rejects v3, while a reader that accepts both versions can share its
v2 decoding and add two statement kinds. Trace v2 binds to the version
automatically emitted for the same input via `source.analysis_digest`.

JSON Schema: [analysis-v3.schema.json](schemas/analysis-v3.schema.json).
