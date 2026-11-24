"""Tests for the one-run Observation stream/snapshot verifier."""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools" / "ci" / "verify_observation_stream.py"


def event(seq: int, kind: str) -> dict:
    return {
        "schema": "marqdo.observation",
        "schema_version": 1,
        "run_id": "run-1",
        "seq": seq,
        "kind": kind,
        "payload": {},
    }


def write(path: pathlib.Path, value) -> pathlib.Path:
    if isinstance(value, list):
        path.write_text(
            "".join(json.dumps(item) + "\n" for item in value), encoding="utf-8"
        )
    else:
        path.write_text(json.dumps(value), encoding="utf-8")
    return path


class VerifyObservationStreamTest(unittest.TestCase):
    def run_script(self, stream, snapshot):
        with tempfile.TemporaryDirectory() as name:
            directory = pathlib.Path(name)
            stream_path = write(directory / "stream.jsonl", stream)
            snapshot_path = write(directory / "snapshot.json", snapshot)
            return subprocess.run(
                [sys.executable, str(SCRIPT), str(stream_path), str(snapshot_path)],
                capture_output=True,
                text=True,
            )

    def test_identical_stream_and_snapshot_pass(self):
        events = [event(0, "stream.opened"), event(1, "run.completed")]
        snapshot = {
            "schema": "marqdo.observation",
            "schema_version": 1,
            "run": {"id": "run-1", "status": "ok", "source": None},
            "capture": {},
            "events": events,
        }
        result = self.run_script(events, snapshot)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_divergent_event_fails(self):
        stream = [event(0, "stream.opened"), event(1, "run.completed")]
        snapshot = {
            "schema": "marqdo.observation",
            "schema_version": 1,
            "run": {"id": "run-1", "status": "ok", "source": None},
            "capture": {},
            "events": [event(0, "stream.opened"), event(1, "run.failed")],
        }
        result = self.run_script(stream, snapshot)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("differ", result.stderr)

    def test_retired_format_fails(self):
        events = [event(0, "stream.opened")]
        retired = dict(events[0], schema="marqdo.execution-events")
        result = self.run_script([retired], {"events": [events[0]]})
        self.assertEqual(result.returncode, 1)
        self.assertIn("current Observation", result.stderr)


if __name__ == "__main__":
    unittest.main()
