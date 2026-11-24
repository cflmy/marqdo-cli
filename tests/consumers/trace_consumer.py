#!/usr/bin/env python3
"""Independent Trace v2 consumer: no Marqdo/Rust imports."""
import hashlib
import json
import sys

if len(sys.argv) != 3:
    raise SystemExit("usage: trace_consumer.py ANALYSIS.json TRACE.json")

with open(sys.argv[1], "rb") as handle:
    analysis_bytes = handle.read()
analysis = json.loads(analysis_bytes)
with open(sys.argv[2], encoding="utf-8") as handle:
    trace = json.load(handle)

if analysis.get("schema") != "marqdo.analysis" or analysis.get("schema_version") != 3:
    raise SystemExit("unsupported analysis")
if trace.get("schema") != "marqdo.trace" or trace.get("schema_version") != 2:
    raise SystemExit("unsupported trace")

modules = {module["id"]: module for module in analysis["modules"]}
if trace["source"]["root_id"] != analysis["project"]["root_id"]:
    raise SystemExit("trace/analysis root mismatch")
expected = "sha256:" + hashlib.sha256(analysis_bytes).hexdigest()
if trace["source"]["analysis_digest"] != expected:
    raise SystemExit("analysis digest mismatch")

truncated = trace.get("truncated")
if truncated is not None and truncated.get("reason") not in ("event_budget", "byte_budget"):
    raise SystemExit("bad truncation reason")

previous = None
events = trace["events"]
for event in events:
    seq = event["seq"]
    if previous is not None and seq <= previous:
        raise SystemExit("sequence is not increasing")
    previous = seq
    span = event.get("span")
    if span is not None and (span["start"] > span["end"]):
        raise SystemExit("bad span")
    module_id = event.get("module_id")
    if module_id is not None and module_id not in modules:
        raise SystemExit("event references an unknown module")
    function_id = event.get("function_id")
    if function_id is not None:
        owner = function_id.rsplit("::", 1)[0]
        if owner not in modules:
            raise SystemExit("function id references an unknown module")

if truncated is not None and events and events[-1].get("kind") != "trace.truncated":
    raise SystemExit("truncated trace has no terminal marker")
print("events=" + str(len(events)))
