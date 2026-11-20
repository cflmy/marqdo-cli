#!/usr/bin/env python3
"""Reproducible P6 baseline profile for selected .mq.md workloads.

Builds the release CLI, runs each workload three times through /usr/bin/time,
and writes a JSON report with wall seconds and peak RSS. This is a baseline
profile, not a claim that P6 optimization is complete.
"""

import json
import os
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
if not target_dir.is_absolute():
    target_dir = ROOT / target_dir
BIN = target_dir / "release" / "marqdo"
WORKLOADS = [
    ("allocator-free-list", "tests/conformance/semantics/55-first-fit-free-list-allocator.mq.md"),
    ("json-parser", "tests/conformance/semantics/62-json-array-parser.mq.md"),
    ("table-sort", "tests/conformance/semantics/65-table-sort-stdlib.mq.md"),
    ("bytecode-vm", "tests/conformance/semantics/63-memory-bytecode-vm.mq.md"),
    ("matrix-multiply", "tests/conformance/semantics/70-matrix-multiply.mq.md"),
    ("http-parser", "tests/conformance/semantics/71-http-parser.mq.md"),
    ("stdlib-print-demo", "stdlib/demo.mq.md"),
]


def run_once(relative: str) -> dict:
    with tempfile.NamedTemporaryFile("r", delete=False) as handle:
        time_path = handle.name
    command = [
        "/usr/bin/time",
        "-f",
        "%e %M",
        "-o",
        time_path,
        str(BIN),
        "run",
        relative,
        "--json",
    ]
    env = dict(os.environ)
    env["MARQDO_STDLIB_DIR"] = str(ROOT / "stdlib")
    started = time.perf_counter()
    output = subprocess.run(
        command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=120
    )
    elapsed = time.perf_counter() - started
    timing = Path(time_path).read_text().split()
    Path(time_path).unlink(missing_ok=True)
    if output.returncode != 0:
        raise RuntimeError(f"{relative} failed: {output.stderr}")
    report = json.loads(output.stdout)
    if not report.get("success"):
        raise RuntimeError(f"{relative} reported failure")
    return {
        "seconds": elapsed,
        "max_rss_kb": int(timing[1]),
    }


def main() -> int:
    subprocess.run(
        ["cargo", "build", "--release", "-p", "marqdo-cli"],
        cwd=ROOT,
        check=True,
    )
    report = {
        "schema": "marqdo.perf-baseline",
        "schema_version": 1,
        "binary": str(BIN.relative_to(ROOT)),
        "iterations": 3,
        "workloads": [],
    }
    for name, relative in WORKLOADS:
        runs = [run_once(relative) for _ in range(3)]
        report["workloads"].append(
            {
                "name": name,
                "source": relative,
                "seconds_median": round(statistics.median(r["seconds"] for r in runs), 4),
                "seconds_min": min(r["seconds"] for r in runs),
                "max_rss_kb_median": int(
                    statistics.median(r["max_rss_kb"] for r in runs)
                ),
            }
        )
    json.dump(report, sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
