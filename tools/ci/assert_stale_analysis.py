#!/usr/bin/env python3
"""Assert that an Observation consumer rejects a drifted Analysis artifact.

The CI contract lane must prove that stale Analysis bytes are not accepted by a
consumer. The check lives in a checked-in script because the CI runner does not
reliably preserve shell quoting inside multiline YAML commands.

usage: assert_stale_analysis.py ANALYSIS EVENTS
"""
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]


def main(argv):
    if len(argv) != 3:
        raise SystemExit("usage: assert_stale_analysis.py ANALYSIS EVENTS")
    analysis_path = pathlib.Path(argv[1])
    events_path = pathlib.Path(argv[2])
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            prefix="marqdo-stale-", suffix=".json", delete=False
        ) as handle:
            handle.write(analysis_path.read_bytes() + b" ")
            temporary = pathlib.Path(handle.name)
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "tests" / "consumers" / "observation_consumer.py"),
                "--analysis",
                str(temporary),
                str(events_path),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            print("stale Analysis was accepted", file=sys.stderr)
            return 1
        print("stale Analysis rejected")
        return 0
    finally:
        if temporary is not None:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass


if __name__ == "__main__":
    sys.exit(main(sys.argv))
