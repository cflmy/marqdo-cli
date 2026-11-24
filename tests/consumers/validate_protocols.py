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
from referencing import Registry, Resource

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCHEMA_DIR = ROOT / "spec" / "schemas"


def load_schema(name):
    path = SCHEMA_DIR / name
    schema = json.loads(path.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return schema


def validator(schema):
    event = load_schema("observation-event.schema.json")
    registry = Registry().with_resource(event["$id"], Resource.from_contents(event))
    return Draft202012Validator(schema, registry=registry)


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
            "usage: validate_protocols.py ANALYSIS.json OBSERVATION.json TOOL.json"
        )
    analysis_path, observation_path, tool_path = map(pathlib.Path, sys.argv[1:])
    analysis = json.loads(analysis_path.read_text(encoding="utf-8"))
    observation = json.loads(observation_path.read_text(encoding="utf-8"))
    tool = json.loads(tool_path.read_text(encoding="utf-8"))

    analysis_schema = "analysis.schema.json"
    require_valid(analysis_schema, analysis, "analysis")
    require_valid("observation-file.schema.json", observation, "observation")
    require_valid("cli-tool.schema.json", tool, "tool")

    # Negative mutations: structure, version and traversal must fail closed.
    bad = copy.deepcopy(analysis)
    bad["schema_version"] = 2
    require_invalid(analysis_schema, bad, "analysis unsupported version")

    bad = copy.deepcopy(analysis)
    bad.pop("modules", None)
    require_invalid(analysis_schema, bad, "analysis missing modules")

    bad = copy.deepcopy(analysis)
    bad["modules"][0]["resources"] = 3
    require_invalid(analysis_schema, bad, "analysis resources integer")

    bad = copy.deepcopy(observation)
    bad["events"][0]["seq"] = "zero"
    require_invalid("observation-file.schema.json", bad, "observation event sequence type")

    bad = copy.deepcopy(observation)
    bad["events"][0]["schema"] = "marqdo.execution-events"
    require_invalid("observation-file.schema.json", bad, "retired event format")
    bad = copy.deepcopy(tool)
    bad["requires"]["analysis"] = [3]
    require_invalid("cli-tool.schema.json", bad, "tool version requirement array")

    bad = copy.deepcopy(tool)
    bad["entrypoint"] = "../outside"
    require_invalid("cli-tool.schema.json", bad, "tool traversal entrypoint")

    bad = copy.deepcopy(tool)
    bad["version"] = "not semver"
    require_invalid("cli-tool.schema.json", bad, "tool invalid version")

    print(
        "validated analysis/observation/tool artifacts "
        f"({len(analysis['modules'])} modules, {len(observation['events'])} events)"
    )


if __name__ == "__main__":
    main()
