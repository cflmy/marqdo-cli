#!/usr/bin/env python3
"""Independent Analysis consumer: no Marqdo/Rust imports."""
import json
import sys

def visit(body, loop_span=None):
    for statement in body:
        kind = statement["kind"]
        if kind == "loop":
            yield from visit(statement["body"], statement["span"])
        elif kind == "branch":
            for arm in statement["arms"]:
                yield from visit(arm["body"], loop_span)
        elif kind in ("break", "continue"):
            if loop_span is None:
                raise SystemExit(f"{kind} outside a projected loop")
            span = statement["span"]
            if not 0 <= loop_span["start"] < span["start"] < span["end"]:
                raise SystemExit(f"invalid {kind} carrier span")
            yield kind, loop_span["start"]
        elif kind not in ("assign", "expression", "return"):
            raise SystemExit(f"unsupported statement kind {kind}")


if sys.argv[2:] not in ([], ["--require-transfers"]) or len(sys.argv) < 2:
    raise SystemExit("usage: analysis_consumer.py ANALYSIS.json [--require-transfers]")

with open(sys.argv[1], encoding="utf-8") as handle:
    analysis = json.load(handle)

if analysis.get("schema") != "marqdo.analysis":
    raise SystemExit("unexpected schema")
if analysis.get("schema_version") != 3:
    raise SystemExit("unsupported schema_version")

project = analysis["project"]
if project.get("identity_kind") not in ("manifest", "workspace_snapshot"):
    raise SystemExit("bad project identity")
if not isinstance(project.get("root_id"), str) or not project["root_id"]:
    raise SystemExit("bad root_id")

functions = []
transfers = []
for module in analysis["modules"]:
    module_id = module["id"]
    if "::" not in module_id or not module_id.startswith(("root:", "dep:", "std:")):
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
        transfers.extend(visit(function["body"]))
        functions.append(function["id"])

if not functions:
    raise SystemExit("no functions")
print("functions=" + str(len(functions)))

if len(sys.argv) == 3 and not transfers:
    raise SystemExit("missing loop transfers")
print(f"transfers={len(transfers)}")
