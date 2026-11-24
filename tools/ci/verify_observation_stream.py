#!/usr/bin/env python3
"""Assert that one Observation run's JSONL stream and JSON snapshot agree.

Usage:
    python3 tools/ci/verify_observation_stream.py STREAM.jsonl SNAPSHOT.json

Exit 0 when the snapshot's events are exactly the stream's events, 1 when they
differ or a document is not a current Observation artifact. Both destinations
must come from the same execution; this check never re-runs the program.
"""
from __future__ import annotations

import json
import pathlib
import sys


def load_events(path: pathlib.Path, *, snapshot: bool) -> list:
    raw = path.read_bytes()
    if snapshot:
        document = json.loads(raw.decode("utf-8"))
        if document.get("schema") != "marqdo.observation":
            raise SystemExit(f"{path}: not a current Observation snapshot")
        events = document.get("events")
    else:
        if raw and not raw.endswith(b"\n"):
            raise SystemExit(f"{path}: interrupted JSONL record")
        events = [json.loads(line) for line in raw.splitlines()]
    if not isinstance(events, list) or not events:
        raise SystemExit(f"{path}: no Observation events")
    for event in events:
        if event.get("schema") != "marqdo.observation":
            raise SystemExit(f"{path}: event is not current Observation")
    return events


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(
            "usage: verify_observation_stream.py STREAM.jsonl SNAPSHOT.json",
            file=sys.stderr,
        )
        return 2
    stream = load_events(pathlib.Path(argv[1]), snapshot=False)
    snapshot = load_events(pathlib.Path(argv[2]), snapshot=True)
    if stream != snapshot:
        for index, (left, right) in enumerate(zip(stream, snapshot)):
            if left != right:
                print(
                    f"stream and snapshot differ at seq {left.get('seq', index)}",
                    file=sys.stderr,
                )
                break
        else:
            print(
                f"stream has {len(stream)} events, snapshot has {len(snapshot)}",
                file=sys.stderr,
            )
        return 1
    print(f"stream and snapshot share {len(stream)} events from one run")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
