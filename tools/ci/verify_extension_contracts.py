#!/usr/bin/env python3
"""Frozen-CLI external-extension evidence generator and validator.

RFC-EXT-0001 §10.2/§11.3: a release candidate must prove that independent
repositories work against an *unchanged* CLI binary. This script:

1. hashes the CLI binary before and after the external suite and refuses to
   emit PASS if the hash moved;
2. records the exact marqdo-cli source SHA and the external fixture repo SHA;
3. runs the fixture repository's own black-box suite with ``MARQDO_BIN`` set to
   the frozen binary (the suite itself must not run cargo or read the CLI
   source tree);
4. validates/normalises the per-case results into the repository evidence
   schema (``docs/evidence/extension/schema.json``);
5. writes ``docs/evidence/extension/<tested_sha>/manifest.json``.

Usage:

    python3 tools/ci/verify_extension_contracts.py \
        --cli target/release/marqdo \
        --fixtures ../marqdo-extension-conformance \
        --out docs/evidence/extension/<sha>/manifest.json

    python3 tools/ci/verify_extension_contracts.py --validate manifest.json

Only Python's standard library is used. No network access is performed by this
script; the external suite documents its own offline cache behaviour.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCHEMA = REPO / "docs" / "evidence" / "extension" / "schema.json"

REQUIRED_RESULTS = [
    "callable_registry",
    "plugin_worker",
    "foreign_kernel",
    "resource_provider_cli",
    "package_pinned_git",
    "tool_analysis_consumer",
    "negative_security",
]

ALLOWED = {"PASS", "FAIL", "UNSUPPORTED", "SKIPPED"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_sha(directory: Path) -> str:
    output = subprocess.run(
        ["git", "-C", str(directory), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return output.stdout.strip()


def os_name() -> str:
    name = platform.system().lower()
    if name.startswith("linux"):
        return "linux"
    if name == "darwin":
        return "macos"
    if name.startswith("windows"):
        return "windows"
    raise SystemExit(f"unsupported OS for extension evidence: {name}")


def validate_manifest(manifest: dict) -> list[str]:
    problems: list[str] = []
    if manifest.get("schema") != "marqdo.extension-evidence":
        problems.append("schema must be marqdo.extension-evidence")
    if manifest.get("schema_version") != 1:
        problems.append("schema_version must be 1")
    for field in ["tested_sha", "fixture_repo_sha"]:
        value = manifest.get(field, "")
        if not (isinstance(value, str) and len(value) == 40 and all(c in "0123456789abcdef" for c in value)):
            problems.append(f"{field} must be a 40-character lowercase git SHA")
    cli = manifest.get("cli_binary_sha256", "")
    if not (isinstance(cli, str) and len(cli) == 64 and all(c in "0123456789abcdef" for c in cli)):
        problems.append("cli_binary_sha256 must be a 64-character lowercase sha256")
    if manifest.get("os") not in {"linux", "macos", "windows"}:
        problems.append("os must be linux, macos or windows")
    results = manifest.get("results")
    if not isinstance(results, dict):
        problems.append("results must be an object")
    else:
        for name in REQUIRED_RESULTS:
            if results.get(name) not in ALLOWED:
                problems.append(f"results.{name} must be one of {sorted(ALLOWED)}")
        for key, value in results.items():
            if key not in REQUIRED_RESULTS:
                problems.append(f"unknown result key {key}")
            if value not in ALLOWED:
                problems.append(f"result {key}={value} is not allowed")
    commands = manifest.get("commands")
    if not isinstance(commands, list) or not commands or not all(isinstance(c, str) for c in commands):
        problems.append("commands must be a non-empty array of strings")
    for field in ["failures", "unverified"]:
        if not isinstance(manifest.get(field), list):
            problems.append(f"{field} must be an array")
    if not problems:
        for name, value in (manifest.get("results") or {}).items():
            if value in {"FAIL"}:
                problems.append(f"result {name} is FAIL")
    return problems


def run_suite(fixtures: Path, cli: Path) -> tuple[bool, dict, list[str]]:
    """Run the external suite and return (ok, results, commands)."""
    results: dict[str, str] = {}
    commands: list[str] = []
    env = dict(os.environ)
    env["MARQDO_BIN"] = str(cli)
    env["MARQDO_FROZEN_CLI"] = str(cli)
    runner = fixtures / "tests" / "run_all.py"
    if not runner.is_file():
        return False, results, commands
    commands.append(f"python3 {runner} (cwd={fixtures})")
    completed = subprocess.run(
        [sys.executable, str(runner)],
        cwd=fixtures,
        env=env,
        capture_output=True,
        text=True,
    )
    print(completed.stdout, end="")
    print(completed.stderr, end="", file=sys.stderr)
    results_path = fixtures / "evidence" / "results.json"
    if results_path.is_file():
        with results_path.open("r", encoding="utf-8") as handle:
            payload = json.load(handle)
        for key, value in (payload.get("results") or {}).items():
            results[key] = str(value).upper()
    return completed.returncode == 0, results, commands


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, help="Absolute/relative frozen marqdo binary")
    parser.add_argument("--fixtures", type=Path, help="External fixture repository root")
    parser.add_argument("--source-repo", type=Path, default=REPO, help="marqdo-cli checkout")
    parser.add_argument("--out", type=Path, help="Evidence manifest output path")
    parser.add_argument("--validate", type=Path, help="Only validate an existing manifest")
    parser.add_argument("--allow-unverified", action="store_true", help="Record UNSUPPORTED/SKIPPED without failing")
    parser.add_argument(
        "--tested-sha",
        help="Require the source checkout HEAD to be exactly this 40-hex SHA before testing",
    )
    parser.add_argument(
        "--allow-dirty-source",
        action="store_true",
        help="Development only: permit a dirty source checkout (never valid for release evidence)",
    )
    args = parser.parse_args()

    if args.validate:
        manifest = json.loads(args.validate.read_text(encoding="utf-8"))
        problems = validate_manifest(manifest)
        if problems:
            for problem in problems:
                print(f"INVALID: {problem}", file=sys.stderr)
            return 1
        print(f"valid extension evidence: {args.validate}")
        return 0

    if not args.cli or not args.fixtures:
        parser.error("--cli and --fixtures are required unless --validate is used")
    cli = args.cli.resolve()
    fixtures = args.fixtures.resolve()
    if not cli.is_file():
        print(f"error: CLI binary not found: {cli}", file=sys.stderr)
        return 2
    if not (fixtures / ".git").exists():
        print(f"error: fixture path is not a git repository: {fixtures}", file=sys.stderr)
        return 2
    if subprocess.run(
        ["git", "-C", str(fixtures), "status", "--porcelain"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip():
        print("error: fixture repository has uncommitted changes", file=sys.stderr)
        return 2

    source_repo = args.source_repo.resolve()
    if not args.allow_dirty_source:
        dirty = subprocess.run(
            ["git", "-C", str(source_repo), "status", "--porcelain"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        if dirty:
            print(
                "error: source checkout has uncommitted changes; release evidence must "
                "name a committed SHA (use --allow-dirty-source only for local development)",
                file=sys.stderr,
            )
            return 2
    cli_before = sha256_file(cli)
    tested_sha = git_sha(source_repo)
    if args.tested_sha and args.tested_sha != tested_sha:
        print(
            f"error: --tested-sha {args.tested_sha} does not match checkout HEAD {tested_sha}",
            file=sys.stderr,
        )
        return 2
    fixture_sha = git_sha(fixtures)
    ok, results, commands = run_suite(fixtures, cli)
    cli_after = sha256_file(cli)
    if cli_before != cli_after:
        print("error: frozen CLI hash changed during the external suite", file=sys.stderr)
        return 1

    failures = [name for name, value in results.items() if value == "FAIL"]
    unverified = [
        name for name, value in results.items() if value in {"UNSUPPORTED", "SKIPPED"}
    ]
    missing = [name for name in REQUIRED_RESULTS if name not in results]
    if missing:
        failures.extend(f"missing:{name}" for name in missing)
    manifest = {
        "schema": "marqdo.extension-evidence",
        "schema_version": 1,
        "tested_sha": tested_sha,
        "cli_binary_sha256": cli_before,
        "fixture_repo_sha": fixture_sha,
        "os": os_name(),
        "arch": platform.machine(),
        "results": {name: results.get(name, "SKIPPED") for name in REQUIRED_RESULTS},
        "commands": commands,
        "failures": failures,
        "unverified": unverified,
    }
    problems = validate_manifest(manifest)
    if unverified and not args.allow_unverified:
        problems.append(
            "UNSUPPORTED/SKIPPED results require --allow-unverified and an explicit report entry"
        )
    if not ok and not failures:
        failures.append("external suite exited non-zero without structured results")
        problems.append("external suite failed")
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        print(json.dumps(manifest, indent=2))
        return 1
    out = args.out or (REPO / "docs" / "evidence" / "extension" / tested_sha / "manifest.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote extension evidence: {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
