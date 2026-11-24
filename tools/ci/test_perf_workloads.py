"""Tests for the performance workload catalogue and correctness digest.

The G4 release gate runs one fixed release binary over nine workloads. A
retired CLI option or a digest that changes per invocation silently turns a
correctness-verified measurement into CORRECTNESS_FAIL, so both are pinned
here.
"""

import importlib.util
import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKLOADS = ROOT / "tools" / "perf" / "workloads.json"
RUNNER = ROOT / "tools" / "perf" / "runner.py"


def load_runner():
    spec = importlib.util.spec_from_file_location("marqdo_perf_runner", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def snapshot(run_id: str, status: str, value, extra_file: str = "/tmp/index.mq.md"):
    return {
        "schema": "marqdo.observation",
        "schema_version": 1,
        "run": {"id": run_id, "status": status, "source": None},
        "capture": {},
        "events": [
            {
                "schema": "marqdo.observation",
                "schema_version": 1,
                "run_id": run_id,
                "seq": 0,
                "kind": "stream.opened",
                "payload": {"producer": {"name": "marqdo", "version": "3.0.0-rc.3"}},
            },
            {
                "schema": "marqdo.observation",
                "schema_version": 1,
                "run_id": run_id,
                "seq": 1,
                "kind": "run.started",
                "payload": {"entry": "e::index.mq.md::main", "started_unix_ms": 1795523668882},
            },
            {
                "schema": "marqdo.observation",
                "schema_version": 1,
                "run_id": run_id,
                "seq": 2,
                "kind": "run.completed",
                "payload": {"status": status, "value": value, "value_type": "Int"},
            },
        ],
    }


class WorkloadCatalogueTest(unittest.TestCase):
    def test_run_and_call_workloads_use_the_current_output_option(self):
        catalogue = json.loads(WORKLOADS.read_text(encoding="utf-8"))
        workloads = catalogue["workloads"]
        self.assertEqual(len(workloads), 9)
        for workload in workloads:
            argv = workload["argv"]
            if workload["kind"] in ("run", "call"):
                self.assertIn(
                    ["--output", "json"],
                    [argv[index : index + 2] for index in range(len(argv))],
                    f"{workload['name']} must request the current Observation snapshot",
                )
                self.assertNotIn(
                    "--json",
                    argv,
                    f"{workload['name']} still passes the retired run/call flag",
                )
            else:
                self.assertIn("--json", argv)


class DigestStabilityTest(unittest.TestCase):
    def setUp(self):
        self.runner = load_runner()

    def test_observation_digest_ignores_run_identity_and_paths(self):
        first = self.runner.wire_digest(snapshot("run-1", "ok", 120))
        second = self.runner.wire_digest(
            snapshot("run-2", "ok", 120, extra_file="/other/host/index.mq.md")
        )
        self.assertEqual(first, second)

    def test_observation_digest_tracks_the_result(self):
        ok = self.runner.wire_digest(snapshot("run-1", "ok", 120))
        wrong = self.runner.wire_digest(snapshot("run-1", "ok", 121))
        failed = self.runner.wire_digest(snapshot("run-1", "error", 120))
        self.assertNotEqual(ok, wrong)
        self.assertNotEqual(ok, failed)

    def test_observation_digest_tracks_the_event_stream(self):
        document = snapshot("run-1", "ok", 120)
        truncated = dict(document)
        truncated["events"] = document["events"][:2]
        self.assertNotEqual(
            self.runner.wire_digest(document), self.runner.wire_digest(truncated)
        )

    def test_check_document_keeps_the_command_digest(self):
        report = {
            "command": "check",
            "success": True,
            "diagnostics": [{"code": "sem.x", "severity": "error", "line": 1, "column": 2, "message": "m"}],
            "stdout": "",
            "stderr": "",
        }
        normalized = self.runner.normalize(report)
        self.assertEqual(normalized["command"], "check")
        self.assertEqual(len(normalized["diagnostics"]), 1)


if __name__ == "__main__":
    unittest.main()
