#!/usr/bin/env python3
"""Prove the CLI and Provider Worker build without any stdlib source (ADR-0070).

The script exports one committed revision with `git archive`, removes the
`stdlib/` tree (there is no ecosystem checkout either), builds
`marqdo-cli` + `marqdo-provider-worker` with `--locked`, and then:

* runs a language-only program through the freshly built CLI (`check` + `call`);
* requires a stdlib-dependent program to fail closed — the debug source
  fallback points at the removed copy, so a success would be a real defect;
* requires no `stdlib/` directory next to the built binary;
* records both binary digests and the source SHA.

Usage:

    python3 tools/ci/independent_build.py
    python3 tools/ci/independent_build.py --offline --keep
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARIES = ("marqdo", "marqdo-provider-worker")
LANGUAGE_ONLY = Path("tests/independent/language-only.mq.md")
STDLIB_DEPENDENT = Path("examples/hello.mq.md")
MISSING_BUNDLE_PATTERN = re.compile(r"stdlib|symbol\.unknown|打印|print", re.IGNORECASE)


class BuildError(Exception):
    pass


def run(
    command: list[str],
    cwd: Path,
    *,
    env: dict[str, str] | None = None,
    timeout: float = 1800,
    check: bool = True,
) -> subprocess.CompletedProcess:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    if check and completed.returncode != 0:
        raise BuildError(
            f"command failed ({completed.returncode}): {' '.join(command)}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_output(repo: Path, *arguments: str) -> bytes:
    completed = subprocess.run(
        ["git", "-C", str(repo), *arguments],
        capture_output=True,
    )
    if completed.returncode != 0:
        raise BuildError(
            f"git {' '.join(arguments)} failed: {completed.stderr.decode(errors='replace')}"
        )
    return completed.stdout


def export_revision(repo: Path, revision: str, destination: Path) -> str:
    try:
        head = git_output(repo, "rev-parse", revision).decode().strip()
        archive = git_output(repo, "archive", "--format=tar", revision)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise BuildError(f"cannot export {revision} from {repo}: {error}") from error
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        try:
            tar.extractall(destination, filter="fully_trusted")
        except TypeError:  # Python < 3.12 without the PEP 706 backport
            tar.extractall(destination)
    return head


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT))
    parser.add_argument("--source", default="HEAD", help="committed revision to export")
    parser.add_argument("--work-dir", default=None, help="explicit work directory (kept)")
    parser.add_argument("--target-dir", default=None, help="cargo target directory (kept)")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--timeout", type=float, default=1800.0)
    args = parser.parse_args(argv)

    repo = Path(args.repo).resolve()
    if args.work_dir:
        work = Path(args.work_dir).resolve()
        if work.exists():
            shutil.rmtree(work)
        keep = True
    else:
        work = Path(tempfile.mkdtemp(prefix="marqdo-independent-build-"))
        keep = args.keep
    target = Path(args.target_dir).resolve() if args.target_dir else work / "target"
    source = work / "source"
    try:
        source_sha = export_revision(repo, args.source, source)
        stdlib = source / "stdlib"
        if not stdlib.is_dir():
            raise BuildError(
                "the exported revision has no stdlib/ tree; the lane must remove a "
                "real Bundle to prove the build does not need it"
            )
        shutil.rmtree(stdlib)
        if stdlib.exists():
            raise BuildError(f"failed to remove {stdlib}")

        env = os.environ.copy()
        env.pop("MARQDO_STDLIB_DIR", None)
        env["CARGO_TARGET_DIR"] = str(target)
        env.setdefault("NO_COLOR", "1")
        command = ["cargo", "build", "--locked", "-p", "marqdo-cli", "-p", "marqdo-provider-worker"]
        if args.offline:
            command.append("--offline")
        run(command, source, env=env, timeout=args.timeout)

        binary = target / "debug" / "marqdo"
        if not binary.is_file():
            raise BuildError(f"the build produced no CLI binary at {binary}")
        if (binary.parent / "stdlib").exists():
            raise BuildError(f"a stdlib/ directory appeared next to {binary}")
        binaries = {}
        for name in BINARIES:
            path = target / "debug" / name
            if not path.is_file():
                raise BuildError(f"the build produced no {name} binary at {path}")
            binaries[name] = {"sha256": sha256_file(path), "size": path.stat().st_size}

        checked = run([str(binary), "check", str(LANGUAGE_ONLY)], source, env=env, timeout=args.timeout)
        called = run(
            [str(binary), "call", str(LANGUAGE_ONLY), "main"],
            source,
            env=env,
            timeout=args.timeout,
        )
        if called.stdout.strip() != "3":
            raise BuildError(
                f"language-only call printed {called.stdout.strip()!r}, expected '3'"
            )
        missing = run(
            [str(binary), "run", str(STDLIB_DEPENDENT)],
            source,
            env=env,
            timeout=args.timeout,
            check=False,
        )
        if missing.returncode == 0:
            raise BuildError(
                "a stdlib-dependent program succeeded without any stdlib Bundle "
                "(a source fallback exists); this is a release blocker"
            )
        if not MISSING_BUNDLE_PATTERN.search(missing.stderr + missing.stdout):
            raise BuildError(
                "the missing-Bundle failure does not name the stdlib/print/symbol "
                f"cause:\n{missing.stderr}"
            )

        summary = {
            "schema": "marqdo.independent-build",
            "schema_version": 1,
            "source_sha": source_sha,
            "source_revision": args.source,
            "copy": str(source),
            "target_dir": str(target),
            "binaries": binaries,
            "checks": {
                "stdlib_source_removed": True,
                "language_only_check": "PASS",
                "language_only_call": "PASS:3",
                "stdlib_dependent_missing_bundle": "PASS",
                "no_sibling_stdlib": True,
            },
            "status": "PASS",
        }
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0
    except BuildError as error:
        print(f"independent-build: error: {error}", file=sys.stderr)
        return 1
    finally:
        if not keep:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
