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

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from analysis_consumer import InvalidAnalysis, validate as validate_analysis

try:
    # jsonschema >= 4.18 resolves the external event schema through
    # `referencing`. The CI image may ship an older jsonschema, so keep the
    # standard resolver fallback: the public consumer must run on both.
    from referencing import Registry, Resource
except ImportError:  # pragma: no cover - exercised by the compatibility shim
    Registry = None
    Resource = None

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCHEMA_DIR = ROOT / "spec" / "schemas"


def load_schema(name):
    path = SCHEMA_DIR / name
    schema = json.loads(path.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return schema


EVENT_SCHEMA_ID = "https://marqdo.dev/schemas/observation-event.schema.json"


def validator(schema):
    event = load_schema("observation-event.schema.json")
    if Registry is not None:
        registry = Registry().with_resource(EVENT_SCHEMA_ID, Resource.from_contents(event))
        return Draft202012Validator(schema, registry=registry)
    # jsonschema < 4.18 (Debian bookworm's python3-jsonschema).
    from jsonschema import RefResolver

    store = {
        EVENT_SCHEMA_ID: event,
        "observation-event.schema.json": event,
    }
    resolver = RefResolver.from_schema(schema, store=store)
    return Draft202012Validator(schema, resolver=resolver)


def require_valid(schema_name, document, label):
    errors = sorted(validator(load_schema(schema_name)).iter_errors(document), key=str)
    if errors:
        raise SystemExit(f"{label}: schema validation failed: {errors[0].message}")


def require_invalid(schema_name, document, label):
    errors = list(validator(load_schema(schema_name)).iter_errors(document))
    if not errors:
        raise SystemExit(f"{label}: expected schema validation to fail")


def semantic_error(document):
    try:
        validate_analysis(document)
    except InvalidAnalysis as error:
        return str(error)
    return None


def require_rejected(schema_name, document, label):
    """A mutation must be rejected by the schema or the semantic validator."""
    schema_errors = list(validator(load_schema(schema_name)).iter_errors(document))
    error = semantic_error(document)
    if not schema_errors and error is None:
        raise SystemExit(f"{label}: expected schema or semantic validation to fail")


def drop(document, path):
    parent = document
    for key in path[:-1]:
        parent = parent[key]
    del parent[path[-1]]


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
    error = semantic_error(analysis)
    if error is not None:
        raise SystemExit(f"analysis: semantic validation failed: {error}")
    require_valid("observation-file.schema.json", observation, "observation")
    require_valid("cli-tool.schema.json", tool, "tool")

    # V01: every newly required field is load-bearing. Removing it must be
    # rejected by the current schema or the semantic validator; an old
    # same-revision artifact is not accepted.
    required_paths = [
        ["project", "entry_module_id"],
        ["project", "manifest_digest"],
        ["project", "extensions"],
        ["modules", 0, "source"],
        ["modules", 0, "document"],
        ["modules", 0, "anchors"],
        ["modules", 0, "units", 0, "path"],
        ["modules", 0, "units", 0, "parent_id"],
        ["modules", 0, "units", 0, "mode_origin"],
        ["modules", 0, "units", 0, "mode_source_unit_id"],
        ["modules", 0, "units", 0, "heading_span"],
        ["modules", 0, "units", 0, "span"],
        ["modules", 0, "functions"],
        ["modules", 0, "references"],
        ["modules", 0, "calls"],
        ["modules", 0, "resources"],
    ]
    for path in required_paths:
        bad = copy.deepcopy(analysis)
        drop(bad, path)
        require_rejected(analysis_schema, bad, "analysis missing " + "/".join(map(str, path)))

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

    # V02/V03: tampered digests and spans must never be accepted.
    bad = copy.deepcopy(analysis)
    bad["modules"][0]["source"] = bad["modules"][0]["source"] + "x"
    require_rejected(analysis_schema, bad, "analysis tampered source")

    bad = copy.deepcopy(analysis)
    bad["modules"][0]["source_digest"] = "0" * 64
    require_rejected(analysis_schema, bad, "analysis tampered source digest")

    bad = copy.deepcopy(analysis)
    bad["modules"][0]["document"]["span"]["end"] = 10**12
    require_rejected(analysis_schema, bad, "analysis out-of-range span")

    bad = copy.deepcopy(analysis)
    bad["modules"].append(copy.deepcopy(bad["modules"][0]))
    require_rejected(analysis_schema, bad, "analysis duplicate module id")

    # V04: heading tree and ownership invariants are semantic.
    units = analysis["modules"][0]["units"]
    if len(units) >= 3:
        bad = copy.deepcopy(analysis)
        bad_units = bad["modules"][0]["units"]
        bad_units[1]["parent_id"] = bad_units[2]["id"]
        bad_units[2]["parent_id"] = bad_units[1]["id"]
        require_rejected(analysis_schema, bad, "analysis parent cycle")
        bad = copy.deepcopy(analysis)
        bad["modules"][0]["units"][1]["mode_source_unit_id"] = "root:missing"
        require_rejected(analysis_schema, bad, "analysis mode source outside ancestors")

    # V05: call, reference and resource targets are semantic.
    module = analysis["modules"][0]
    function_ids = {fn["id"] for module_iter in analysis["modules"] for fn in module_iter["functions"]}
    for call in module["calls"]:
        if call["state"] == "function":
            bad = copy.deepcopy(analysis)
            for candidate in bad["modules"][0]["calls"]:
                if candidate["span"] == call["span"] and candidate["function"] == call["function"]:
                    candidate["target"] = "root:missing::missing"
            require_rejected(analysis_schema, bad, "analysis missing call target")
            break
    references = [ref for module_iter in analysis["modules"] for ref in module_iter["references"]]
    for reference in references:
        if reference["state"] == "resolved" and reference["target_unit"] is not None:
            bad = copy.deepcopy(analysis)
            for module_iter in bad["modules"]:
                for candidate in module_iter["references"]:
                    if (
                        candidate["span"] == reference["span"]
                        and candidate["source"] == reference["source"]
                    ):
                        candidate["target_unit"] = "root:missing#missing"
            require_rejected(analysis_schema, bad, "analysis missing reference unit")
            break
    for resource in module["resources"]:
        bad = copy.deepcopy(analysis)
        bad["modules"][0]["resources"][0]["content_digest"] = "0" * 64
        require_rejected(analysis_schema, bad, "analysis tampered resource digest")
        break

    bad = copy.deepcopy(analysis)
    bad["diagnostics"].append(
        {
            "module": module["id"],
            "relative_path": module["relative_path"],
            "span": None,
            "line": None,
            "column": None,
            "severity": "error",
            "code": "test.injected",
            "message": "injected error",
            "hints": [],
        }
    )
    require_rejected(analysis_schema, bad, "analysis successful output with an error")

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
