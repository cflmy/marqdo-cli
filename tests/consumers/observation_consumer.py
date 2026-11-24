#!/usr/bin/env python3
"""Consume real Marqdo observation output using public contracts only."""
import argparse
import base64
import binascii
import hashlib
import json
import pathlib
import sys

SCHEMAS = pathlib.Path(__file__).resolve().parents[2] / "spec" / "schemas"
EVENT_SCHEMA = json.loads((SCHEMAS / "observation-event.schema.json").read_text())
SCHEMA = "marqdo.observation"
KINDS = set(EVENT_SCHEMA["properties"]["kind"]["enum"])
TERMINALS = {"run.completed": "ok", "run.failed": "error", "run.rejected": "rejected"}


class Invalid(Exception):
    pass


def current_document(value):
    return (
        isinstance(value, dict)
        and value.get("schema") == SCHEMA
        and type(value.get("schema_version")) is int
        and value["schema_version"] == 1
    )


def check_source(source, events, analysis_path, source_root):
    raw = analysis_path.read_bytes()
    report = json.loads(raw)
    if report.get("schema") != "marqdo.analysis" or report.get("schema_version") != 3:
        raise Invalid("unsupported Analysis format")
    expected = "sha256:" + hashlib.sha256(raw).hexdigest()
    if source is None or source["analysis_digest"] != expected:
        raise Invalid("Analysis digest mismatch")
    if source["root_id"] != report["project"]["root_id"]:
        raise Invalid("Analysis project identity mismatch")

    modules = {module["id"]: module for module in report["modules"]}
    checked_sources = {}
    for event in events:
        payload = event["payload"]
        functions = [payload["function_id"]] if "function_id" in payload else []
        for function in functions:
            if function.rsplit("::", 1)[0] not in modules:
                raise Invalid("unknown function module")
        if "module_id" not in payload:
            continue
        module_id = payload["module_id"]
        if module_id not in modules:
            raise Invalid("unknown event module")
        span = payload.get("span")
        if span is not None and not 0 <= span["start"] <= span["end"]:
            raise Invalid("invalid event span")
        if source_root is None or not module_id.startswith("root:"):
            continue
        if module_id not in checked_sources:
            module = modules[module_id]
            path = (source_root / module["relative_path"]).resolve()
            if not path.is_relative_to(source_root):
                raise Invalid("root source path escaped its checkout")
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() != module["source_digest"]:
                raise Invalid(f"source digest mismatch for {module['relative_path']}")
            checked_sources[module_id] = data
        if span is not None:
            data = checked_sources[module_id]
            if span["end"] > len(data):
                raise Invalid("event span exceeds source length")
            try:
                data[span["start"]:span["end"]].decode("utf-8")
            except UnicodeDecodeError as error:
                raise Invalid("event span splits UTF-8") from error


def check_chunk(payload):
    if payload["encoding"] != "base64":
        raise Invalid("unsupported chunk encoding")
    try:
        raw = base64.b64decode(payload["data_base64"], validate=True)
    except (binascii.Error, ValueError) as error:
        raise Invalid("invalid base64 chunk") from error
    if len(raw) != payload["byte_count"]:
        raise Invalid("chunk byte count mismatch")
    return len(raw)


def consume(events, snapshot=None, analysis=None, source_root=None):
    if not isinstance(events, list) or not events:
        raise Invalid("expected an opening event")
    first = events[0]
    if not current_document(first) or first.get("kind") != "stream.opened":
        raise Invalid("expected current Observation opening event")
    run_id = first.get("run_id")
    if not isinstance(run_id, str) or not run_id:
        raise Invalid("invalid run identity")
    source = None
    terminal = None
    truncation = None
    chunks = 0
    stack = []
    seen = set()
    for seq, event in enumerate(events):
        if (
            not current_document(event)
            or event.get("run_id") != run_id
            or type(event.get("seq")) is not int
            or event["seq"] != seq
            or event.get("kind") not in KINDS
            or not isinstance(event.get("payload"), dict)
        ):
            raise Invalid(f"invalid event envelope at sequence {seq}")
        kind = event["kind"]
        payload = event["payload"]
        if terminal is not None:
            raise Invalid("event after terminal")
        if seq == 0:
            continue
        if kind == "run.started" and seq == 1:
            source = payload["source"]
            if not isinstance(source, dict):
                raise Invalid("missing source binding")
            continue
        if kind == "run.rejected" and seq == 1:
            terminal = TERMINALS[kind]
        elif source is None or kind in ("run.started", "stream.opened", "run.rejected"):
            raise Invalid("invalid run lifecycle")
        elif kind in TERMINALS:
            terminal = TERMINALS[kind]
        elif kind in ("stdout.chunk", "stderr.chunk"):
            chunks += check_chunk(payload)
        elif kind == "observation.truncated":
            if truncation is not None:
                raise Invalid("duplicate truncation marker")
            truncation = payload
        elif truncation is not None:
            raise Invalid("structural event after truncation")
        elif kind == "call.enter":
            frame = payload["frame_id"]
            if frame in seen or payload["parent_frame_id"] != (stack[-1] if stack else None):
                raise Invalid("invalid call frame parent or reused frame identity")
            seen.add(frame)
            stack.append(frame)
        elif kind == "call.exit":
            if not stack or payload["frame_id"] != stack.pop():
                raise Invalid("mismatched call exit")
        elif payload.get("frame_id") is not None and (
            not stack or payload["frame_id"] != stack[-1]
        ):
            raise Invalid("observation outside its owning frame")
        if terminal is not None and payload["status"] != terminal:
            raise Invalid("terminal status mismatch")

    if terminal is not None and truncation is None and stack:
        raise Invalid("unclosed frame")
    if snapshot is not None:
        run = snapshot["run"]
        capture = snapshot["capture"]
        if (
            run["id"] != run_id
            or run["source"] != source
            or run["status"] != (terminal or "incomplete")
            or capture["truncated"] != truncation
            or {key: value for key, value in capture.items() if key != "truncated"}
            != first["payload"]["observation"]
        ):
            raise Invalid("snapshot metadata disagrees with its events")
    if analysis is not None:
        check_source(source, events, analysis, source_root)
    if terminal is None:
        return 3, f"incomplete: {len(events)} events, no terminal"
    return 0, f"{terminal}: {len(events)} events, {chunks} output bytes"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="path or - for stdin")
    parser.add_argument("--snapshot", action="store_true", help="read the JSON snapshot root")
    parser.add_argument("--analysis", type=pathlib.Path)
    parser.add_argument("--validate-schema", action="store_true")
    parser.add_argument("--source-root", type=lambda path: pathlib.Path(path).resolve())
    args = parser.parse_args()
    try:
        text = sys.stdin.read() if args.input == "-" else pathlib.Path(args.input).read_text()
        if args.snapshot:
            snapshot = json.loads(text)
            if not current_document(snapshot):
                raise Invalid("unsupported Observation snapshot")
            events = snapshot["events"]
        else:
            snapshot = None
            if text and not text.endswith("\n"):
                print("incomplete: interrupted JSONL record", file=sys.stderr)
                return 3
            events = [json.loads(line) for line in text.split("\n")[:-1]]
        if args.validate_schema:
            from validate_protocols import require_valid

            if snapshot is not None:
                require_valid("observation-file.schema.json", snapshot, "snapshot")
            else:
                for index, event in enumerate(events):
                    require_valid("observation-event.schema.json", event, f"seq {index}")
        status, message = consume(events, snapshot, args.analysis, args.source_root)
        print(message)
        return status
    except (Invalid, KeyError, TypeError, ValueError, OSError) as error:
        print(f"invalid Observation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
