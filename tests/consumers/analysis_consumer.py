#!/usr/bin/env python3
"""Independent Analysis v1 consumer: no Marqdo/Rust imports."""
import json
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: analysis_consumer.py ANALYSIS.json")

with open(sys.argv[1], encoding="utf-8") as handle:
    analysis = json.load(handle)

if analysis.get("schema") != "marqdo.analysis":
    raise SystemExit("unexpected schema")
if analysis.get("schema_version") != 1:
    raise SystemExit("unsupported schema_version")

functions = []
for module in analysis["modules"]:
    if not module["id"].startswith("root:default::"):
        raise SystemExit("unexpected module identity")
    if not isinstance(module["source_digest"], str) or len(module["source_digest"]) != 64:
        raise SystemExit("bad source digest")
    for function in module["functions"]:
        body = json.dumps(function["body"], ensure_ascii=False)
        functions.append((function["id"], body))

if not functions:
    raise SystemExit("no functions")
print("functions=" + str(len(functions)))
