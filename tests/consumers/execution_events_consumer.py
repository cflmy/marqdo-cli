#!/usr/bin/env python3
"""Black-box consumer for Marqdo Execution Events v1 (RFC-OBS-0001).

This is an independent consumer: it imports only the Python standard library
and the `jsonschema` package, never the Rust workspace, and it consumes the
output of a released `marqdo` binary (or a checked-in fixture).

Usage:
    execution_events_consumer.py EVENTS.jsonl
    execution_events_consumer.py -            # read stdin
    execution_events_consumer.py --schema-check FILE.jsonl

Checks:
  * every line is one JSON object validated against
    `spec/schemas/execution-events-v1.schema.json`;
  * `stream.opened` is first (seq 0), `run.started` occurs at most once,
    exactly one terminal event on a complete stream, nothing follows it;
  * `seq` is contiguous and strictly increasing;
  * `run_id` and protocol/schema are consistent across lines;
  * `stdout.chunk`/`stderr.chunk` base64 decodes and `byte_count` matches;
  * an unknown schema major is rejected, never treated as v1.

Exit status 0 means the stream validated. 2 means invalid input, 3 means an
incomplete stream (no terminal event; a legal boundary after SIGKILL/EOF).
"""
import base64
import binascii
import json
import pathlib
import sys

from jsonschema import Draft202012Validator

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCHEMA_PATH = ROOT / "spec" / "schemas" / "execution-events-v1.schema.json"
SCHEMA = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA)

SCHEMA_NAME = "marqdo.execution-events"
SCHEMA_VERSION = 1
TERMINALS = {"run.completed", "run.failed", "run.rejected"}


class Invalid(Exception):
    pass


def parse_line(number, raw):
    try:
        event = json.loads(raw)
    except json.JSONDecodeError as error:
        raise Invalid(f"line {number}: not JSON: {error}") from error
    if not isinstance(event, dict):
        raise Invalid(f"line {number}: event is not a JSON object")
    errors = sorted(VALIDATOR.iter_errors(event), key=str)
    if errors:
        raise Invalid(f"line {number}: schema: {errors[0].message}")
    return event


def check_chunk(event):
    payload = event["payload"]
    if payload.get("encoding") != "base64":
        raise Invalid(f"seq {event['seq']}: unsupported chunk encoding")
    try:
        data = base64.b64decode(payload["data_base64"], validate=True)
    except (binascii.Error, ValueError) as error:
        raise Invalid(f"seq {event['seq']}: data_base64 is not valid base64: {error}") from error
    if len(data) != payload["byte_count"]:
        raise Invalid(
            f"seq {event['seq']}: byte_count={payload['byte_count']} but decoded {len(data)} bytes"
        )
    return data


def consume(lines, require_terminal=True):
    events = [parse_line(number, raw) for number, raw in enumerate(lines, start=1)]
    if not events:
        raise Invalid("empty stream")

    first = events[0]
    if first["kind"] != "stream.opened":
        raise Invalid("first event must be stream.opened")
    if first["seq"] != 0:
        raise Invalid("stream.opened must have seq 0")
    if first["schema"] != SCHEMA_NAME or first["schema_version"] != SCHEMA_VERSION:
        raise Invalid("stream.opened does not negotiate marqdo.execution-events v1")

    run_id = first["run_id"]
    chunks = bytearray()
    terminal = None
    started = 0
    for index, event in enumerate(events):
        seq = event["seq"]
        if seq != index:
            raise Invalid(f"seq {seq} at line {index + 1}; expected {index}")
        if event["schema"] != SCHEMA_NAME or event["schema_version"] != SCHEMA_VERSION:
            raise Invalid(f"seq {seq}: inconsistent schema negotiation")
        if event["run_id"] != run_id:
            raise Invalid(f"seq {seq}: run_id changed within one stream")
        if terminal is not None:
            raise Invalid(f"seq {seq}: event follows terminal {terminal}")
        if event["kind"] == "run.started":
            started += 1
            if started > 1:
                raise Invalid("more than one run.started")
        if event["kind"] in TERMINALS:
            terminal = event["kind"]
        if event["kind"] in ("stdout.chunk", "stderr.chunk"):
            chunks.extend(check_chunk(event))

    if require_terminal and terminal is None:
        return 3, f"incomplete stream: {len(events)} events, no terminal event"
    if terminal == "run.completed" and started != 1:
        raise Invalid("run.completed requires exactly one preceding run.started")
    if terminal == "run.failed" and started != 1:
        raise Invalid("run.failed requires exactly one preceding run.started")
    if terminal == "run.rejected" and started != 0:
        raise Invalid("run.rejected must not be preceded by run.started")
    return 0, (
        f"ok: {len(events)} events, terminal={terminal}, "
        f"stdout+stderr bytes={len(chunks)}"
    )


def main(argv):
    if len(argv) != 2:
        raise SystemExit(__doc__)
    target = argv[1]
    if target == "-":
        lines = [line for line in sys.stdin.read().splitlines() if line.strip()]
    else:
        text = pathlib.Path(target).read_text(encoding="utf-8")
        lines = [line for line in text.splitlines() if line.strip()]
    try:
        status, message = consume(lines)
    except Invalid as error:
        raise SystemExit(f"execution events consumer: INVALID: {error}")
    print(f"execution events consumer: {message}")
    raise SystemExit(status)


if __name__ == "__main__":
    main(sys.argv)
