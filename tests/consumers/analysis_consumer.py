#!/usr/bin/env python3
"""Independent Analysis v2 consumer: no Marqdo/Rust imports."""
import json
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: analysis_consumer.py ANALYSIS.json")

with open(sys.argv[1], encoding="utf-8") as handle:
    analysis = json.load(handle)

if analysis.get("schema") != "marqdo.analysis":
    raise SystemExit("unexpected schema")
if analysis.get("schema_version") != 2:
    raise SystemExit("unsupported schema_version")

project = analysis["project"]
if project.get("identity_kind") not in ("manifest", "workspace_snapshot"):
    raise SystemExit("bad project identity")
if not isinstance(project.get("root_id"), str) or not project["root_id"]:
    raise SystemExit("bad root_id")

functions = []
for module in analysis["modules"]:
    if not module["id"].startswith("root:"):
        raise SystemExit("unexpected module identity")
    if not isinstance(module["source_digest"], str) or len(module["source_digest"]) != 64:
        raise SystemExit("bad source digest")
    resources = module.get("resources")
    if not isinstance(resources, list):
        raise SystemExit("resources must be an array")
    seen = set()
    for resource in resources:
        if resource.get("id") in seen:
            raise SystemExit("duplicate resource id")
        seen.add(resource.get("id"))
        if resource.get("kind") not in ("formula", "code"):
            raise SystemExit("unknown resource kind")
        if resource.get("module_id") != module["id"]:
            raise SystemExit("resource module mismatch")
        if not isinstance(resource.get("content_digest"), str) or len(resource["content_digest"]) != 64:
            raise SystemExit("bad resource digest")
        span = resource.get("span") or {}
        if not isinstance(span.get("start"), int) or not isinstance(span.get("end"), int):
            raise SystemExit("bad resource span")
    for function in module["functions"]:
        body = json.dumps(function["body"], ensure_ascii=False)
        functions.append((function["id"], body))

if not functions:
    raise SystemExit("no functions")
print("functions=" + str(len(functions)))
