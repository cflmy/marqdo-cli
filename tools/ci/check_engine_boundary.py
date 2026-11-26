#!/usr/bin/env python3
"""Engine independence + differential execution gate (issue #13 §4.3-4.4).

1. Builds the real independent consumer in ``tests/embedding`` with its own
   lock file and workspace.
2. Runs it: the same Markdown program goes through the real Frontend, static
   call binding and Runtime, with real fuel/stack/deadline/memory limits and
   explicit resource refusal.
3. Builds the real CLI and executes the same program, then compares the
   terminal Value Wire encoding with the consumer result.
4. Delegates the Cargo-metadata dependency-direction checks to
   ``check_dependency_boundaries.py``.

This is a build-and-run gate, not a source grep. It never mocks the interpreter
and never constructs an expected value in place of execution.

Usage:
    python3 tools/ci/check_engine_boundary.py [--offline]
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CONSUMER = REPO_ROOT / "tests" / "embedding"
PROGRAM = CONSUMER / "program.mq.md"
FORBIDDEN_RUNTIME_EDGES = {
    "marqdo-host",
    "marqdo-sandbox",
    "marqdo-foreign",
    "libloading",
    "rustls",
}


class GateError(Exception):
    pass


def tool(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise GateError(f"`{name}` is not on PATH")
    return path


def run(
    command: list[str],
    *,
    check: bool = True,
    cwd: Path = REPO_ROOT,
) -> subprocess.CompletedProcess:
    result = subprocess.run(
        command, cwd=cwd, capture_output=True, text=True, check=False
    )
    if check and result.returncode != 0:
        raise GateError(
            f"`{' '.join(command)}` failed with {result.returncode}:\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return result


def build_dependencies(package: str, offline: bool) -> set[str]:
    """Parse `cargo tree -e normal,build` package names for one package."""
    command = [tool("cargo"), "tree", "-p", package, "-e", "normal,build", "--prefix", "none"]
    if offline:
        command.append("--offline")
    result = run(command)
    names: set[str] = set()
    for line in result.stdout.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        # Lines look like `marqdo-value v3.0.0-rc.4 (/path)` or `serde v1.0.1`.
        name = stripped.split(" v", 1)[0].strip()
        if name:
            names.add(name)
    return names


def parse_consumer(stdout: str) -> tuple[dict[str, str], dict[str, str]]:
    results: dict[str, str] = {}
    errors: dict[str, str] = {}
    for line in stdout.splitlines():
        if line.startswith("result "):
            _, name, encoded = line.split(" ", 2)
            results[name] = encoded
        elif line.startswith("error "):
            _, name, code = line.split(" ", 2)
            errors[name] = code
    return results, errors


def terminal_value(snapshot: dict) -> object:
    event = snapshot["events"][-1]
    if event["kind"] != "run.completed":
        raise GateError(f"CLI did not complete: {event}")
    return event["payload"]["value"]


def cli_value(binary: Path, function: str, args: list[str]) -> object:
    command = [
        str(binary),
        "call",
        str(PROGRAM),
        function,
        *args,
        "--output",
        "json",
    ]
    result = run(command)
    return terminal_value(json.loads(result.stdout))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    arguments = parser.parse_args()
    offline_flag = ["--offline"] if arguments.offline else []
    try:
        # Dependency direction is a separate, fast check.
        dependency_command = [sys.executable, str(REPO_ROOT / "tools/ci/check_dependency_boundaries.py")]
        dependency_command += offline_flag
        run(dependency_command)

        cargo = tool("cargo")
        run([cargo, "build", "--locked", *offline_flag, "--manifest-path", str(CONSUMER / "Cargo.toml")])
        consumer = CONSUMER / "target" / "debug" / "embedding-consumer"
        if not consumer.is_file():
            raise GateError(f"embedding consumer binary not found: {consumer}")
        consumer_run = run([str(consumer)])
        results, errors = parse_consumer(consumer_run.stdout)

        expected_errors = {
            "fuel": "resource.exhausted",
            "stack": "resource.exhausted",
            "deadline": "resource.exhausted",
            "oob": "runtime.index",
            "file_denied": "permission.denied",
            "file_unsupported": "resource.unsupported",
        }
        for name, code in expected_errors.items():
            if errors.get(name) != code:
                raise GateError(
                    f"consumer negative case `{name}` expected `{code}`, got "
                    f"`{errors.get(name)}` (stderr: {consumer_run.stderr})"
                )

        # A real CLI invocation on the same file and arguments must produce the
        # identical terminal Value Wire encoding.
        run([cargo, "build", "--locked", *offline_flag, "-p", "marqdo-cli"])
        cli = REPO_ROOT / "target" / "debug" / "marqdo"
        if not cli.is_file():
            raise GateError(f"CLI binary not found: {cli}")
        comparisons = [
            ("sum_to", ["--arg", "n=10"]),
            ("types", ["--arg", "seed=7"]),
        ]
        for function, args in comparisons:
            if function not in results:
                raise GateError(f"consumer did not report a `{function}` result")
            from_cli = cli_value(cli, function, args)
            from_consumer = json.loads(results[function])
            if from_cli != from_consumer:
                raise GateError(
                    f"differential mismatch for `{function}`: CLI={from_cli!r} "
                    f"consumer={from_consumer!r}"
                )
            print(f"PASS differential value `{function}` {json.dumps(from_cli, ensure_ascii=False)}")

        runtime_dependencies = build_dependencies("marqdo-runtime", arguments.offline)
        leaked = sorted(FORBIDDEN_RUNTIME_EDGES & runtime_dependencies)
        if leaked:
            raise GateError(
                "marqdo-runtime normal/build tree contains forbidden dependencies: "
                + ", ".join(leaked)
            )
        print(
            "PASS runtime build tree has no native host/loader dependency "
            f"({len(runtime_dependencies)} packages)"
        )
        print("PASS embedding-consumer " + consumer_run.stdout.strip().splitlines()[-1])
    except GateError as error:
        print(f"FAIL engine-boundary: {error}", file=sys.stderr)
        return 1
    print("PASS engine-boundary")
    return 0


if __name__ == "__main__":
    sys.exit(main())
