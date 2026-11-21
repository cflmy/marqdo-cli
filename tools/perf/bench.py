#!/usr/bin/env python3
"""Deprecated alias for tools/perf/runner.py.

The old three-run profile did not verify workload correctness and is not a
performance gate. Use runner.py; this wrapper keeps old local commands working.
"""

import runpy
import sys
from pathlib import Path

print(
    "warning: tools/perf/bench.py is a compatibility shim; "
    "use tools/perf/runner.py for correctness-verified measurements",
    file=sys.stderr,
)
sys.argv[0] = str(Path(__file__).with_name("runner.py"))
runpy.run_path(sys.argv[0], run_name="__main__")
