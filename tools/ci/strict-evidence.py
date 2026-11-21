#!/usr/bin/env python3
"""Emit a machine-checkable G-C strict-security evidence document.

The script never invents measurements. Every field comes from the running
host, the checked-out (or archived) source, or an operator-supplied value that
is recorded as such. It is called by `tests/security/run-lane.sh` and by the
self-hosted security workflow.

Schema: marqdo.strict-evidence.v1 (fields may only be added).
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

SCHEMA = "marqdo.strict-evidence.v1"


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace(
        "+00:00", "Z"
    )


def command_output(command: list[str]) -> str:
    try:
        return subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=20,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def git_head(repo: Path) -> str | None:
    if not (repo / ".git").exists():
        return None
    value = command_output(["git", "-C", str(repo), "rev-parse", "HEAD"])
    return value or None


def cgroup2(repo: Path) -> str:
    for candidate in (Path("/proc/self/mountinfo"),):
        try:
            for line in candidate.read_text().splitlines():
                if " - cgroup2 " in line:
                    return line.split(" - ", 1)[1].strip()
        except OSError:
            pass
    return "unavailable"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="source checkout root")
    parser.add_argument("--out", required=True, help="evidence JSON output path")
    parser.add_argument("--outcome", required=True, choices=["PASS", "FAIL"])
    parser.add_argument("--runner-kind", default=os.environ.get("MARQDO_RUNNER_KIND", "disposable-vm"))
    parser.add_argument("--run-url", default=os.environ.get("MARQDO_RUN_URL", ""))
    parser.add_argument(
        "--image-sha256",
        default=os.environ.get("MARQDO_RUNNER_IMAGE_SHA256", ""),
        help="immutable VM/runner image digest, recorded verbatim",
    )
    parser.add_argument("--started", default="", help="UTC start stamp")
    parser.add_argument("--finished", default="", help="UTC finish stamp")
    parser.add_argument("--log", default="", help="lane log path or URL")
    parser.add_argument(
        "--source-archive",
        default=os.environ.get("MARQDO_SOURCE_ARCHIVE", ""),
        help="source archive used by the lane, when not a git checkout",
    )
    parser.add_argument(
        "--source-archive-sha256",
        default=os.environ.get("MARQDO_SOURCE_ARCHIVE_SHA256", ""),
        help="expected sha256 of the source archive",
    )
    parser.add_argument(
        "--tested-sha",
        default=os.environ.get("MARQDO_TESTED_SHA", ""),
        help="expected commit SHA (required when the checkout has no .git)",
    )
    parser.add_argument("--command", default="./tests/security/run-lane.sh")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    repo = Path(args.repo).resolve()
    head = git_head(repo)
    tested_sha = head or args.tested_sha
    verified = head is not None
    if not tested_sha:
        print("error: no git checkout and no MARQDO_TESTED_SHA/--tested-sha", file=sys.stderr)
        return 2
    if head and args.tested_sha and head != args.tested_sha:
        print(
            f"error: checkout HEAD {head} does not match expected {args.tested_sha}",
            file=sys.stderr,
        )
        return 2

    archive_sha = ""
    if args.source_archive:
        archive = Path(args.source_archive)
        if not archive.is_file():
            print(f"error: source archive does not exist: {archive}", file=sys.stderr)
            return 2
        archive_sha = sha256_file(archive)
        if args.source_archive_sha256 and args.source_archive_sha256 != archive_sha:
            print(
                f"error: source archive digest {archive_sha} does not match "
                f"the expected {args.source_archive_sha256}",
                file=sys.stderr,
            )
            return 2

    uid = os.getuid() if hasattr(os, "getuid") else -1
    document = {
        "schema": SCHEMA,
        "tested_sha": tested_sha,
        "tested_sha_source": "git-rev-parse" if verified else "operator-env",
        "tested_sha_verified": verified,
        "source_archive_sha256": archive_sha,
        "run": {
            "command": args.command,
            "outcome": args.outcome,
            "runner_kind": args.runner_kind,
            "run_url": args.run_url,
            "runner_image_sha256": args.image_sha256,
            "lane_log": args.log,
            "started_utc": args.started or utc_now(),
            "finished_utc": args.finished or utc_now(),
        },
        "environment": {
            "uname": platform.platform(),
            "kernel": platform.release(),
            "machine": platform.machine(),
            "cgroup2": cgroup2(repo),
            "container": command_output(["systemd-detect-virt", "--container"]) or "none-detected",
            "uid": uid,
            "user": os.environ.get("USER", ""),
            "root": uid == 0,
            "bwrap_version": command_output(["bwrap", "--version"]),
            "rustc": command_output(["rustc", "-V"]),
            "cargo": command_output(["cargo", "-V"]),
            "python": platform.python_version(),
            "cc": command_output(["cc", "--version"]).splitlines()[:1],
            "source_archive_sha256_matches": not (
                args.source_archive_sha256
                and archive_sha
                and args.source_archive_sha256 != archive_sha
            ),
        },
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    temporary = out.with_suffix(out.suffix + ".tmp")
    temporary.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(out)
    print(f"strict evidence: {out}")
    if uid == 0 and args.outcome == "PASS":
        print(
            "error: a root-only PASS is not valid G-C evidence",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
