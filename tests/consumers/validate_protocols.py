#!/usr/bin/env python3
"""Validate real Marqdo protocol artifacts against their JSON Schemas.

This is an independent consumer: it imports only Python's standard library and
the `jsonschema` package, never the Rust workspace.
"""
import copy
import json
import pathlib
import sys

from jsonschema import Draft202012Validator

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCHEMA_DIR = ROOT / "spec" / "schemas"


def load_schema(name):
    path = SCHEMA_DIR / name
    schema = json.loads(path.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return schema


def validator(schema):
    return Draft202012Validator(schema)


def require_valid(schema_name, document, label):
    errors = sorted(validator(load_schema(schema_name)).iter_errors(document), key=str)
    if errors:
        raise SystemExit(f"{label}: schema validation failed: {errors[0].message}")


def require_invalid(schema_name, document, label):
    errors = list(validator(load_schema(schema_name)).iter_errors(document))
    if not errors:
        raise SystemExit(f"{label}: expected schema validation to fail")


def main():
    if len(sys.argv) != 4:
        raise SystemExit(
            "usage: validate_protocols.py ANALYSIS.json TRACE.json TOOL.json"
        )
    analysis_path, trace_path, tool_path = map(pathlib.Path, sys.argv[1:])
    analysis = json.loads(analysis_path.read_text(encoding="utf-8"))
    trace = json.loads(trace_path.read_text(encoding="utf-8"))
    tool = json.loads(tool_path.read_text(encoding="utf-8"))

    require_valid("analysis-v2.schema.json", analysis, "analysis")
    require_valid("trace-v2.schema.json", trace, "trace")
    require_valid("tool-v1.schema.json", tool, "tool")

    # Negative mutations: structure, version and traversal must fail closed.
    bad = copy.deepcopy(analysis)
    bad["schema_version"] = 999
    require_invalid("analysis-v2.schema.json", bad, "analysis unsupported version")

    bad = copy.deepcopy(analysis)
    bad.pop("modules", None)
    require_invalid("analysis-v2.schema.json", bad, "analysis missing modules")

    bad = copy.deepcopy(analysis)
    bad["modules"][0]["resources"] = 3
    require_invalid("analysis-v2.schema.json", bad, "analysis resources integer")

    bad = copy.deepcopy(trace)
    bad["events"][0]["seq"] = "zero"
    require_invalid("trace-v2.schema.json", bad, "trace event sequence type")

    bad = copy.deepcopy(trace)
    bad["truncated"] = {"reason": "made_up", "limit": 1, "written": 1}
    require_invalid("trace-v2.schema.json", bad, "trace truncation reason")

    bad = copy.deepcopy(tool)
    bad["entrypoint"] = "../outside"
    require_invalid("tool-v1.schema.json", bad, "tool traversal entrypoint")

    bad = copy.deepcopy(tool)
    bad["version"] = "not semver"
    require_invalid("tool-v1.schema.json", bad, "tool invalid version")

    print(
        "validated analysis/trace/tool artifacts "
        f"({len(analysis['modules'])} modules, {len(trace['events'])} events)"
    )


if __name__ == "__main__":
    main()
