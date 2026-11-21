#!/usr/bin/env python3
"""Build the Marqdo release archive from a clean, pinned commit.

The archive is deterministic (sorted entries, commit timestamp, uid/gid 0) and
contains the CLI, the provider worker helper (same directory, same target), the
licenses/changelog/compatibility matrix, the public specs, the developer docs,
the runnable examples and the public protocol consumers.

It refuses to package a dirty tree, a version that does not match the Cargo
workspace, or a HEAD that differs from `--source-sha`, and it fails if the
example extension artifacts cannot be built on the host. Nothing here publishes
anything; tagging is a separate reviewed step.

Usage (from the repository root, after
`cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker`):

    python3 tools/release/package.py \
        --source-sha "$(git rev-parse HEAD)" \
        --version 3.0.0-beta.1
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARIES = ("marqdo", "marqdo-provider-worker")


def fail(message: str) -> "NoReturn":
    print(f"package: error: {message}", file=sys.stderr)
    raise SystemExit(1)


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(command, check=True, text=True, **kwargs)


def git(*args: str) -> str:
    return run(["git", *args], cwd=ROOT, capture_output=True).stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def host_target() -> str:
    out = run(["rustc", "-vV"], capture_output=True).stdout
    for line in out.splitlines():
        if line.startswith("host: "):
            return line[len("host: ") :]
    fail("rustc -vV has no host line")


def workspace_version() -> str:
    metadata = json.loads(
        run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
            cwd=ROOT,
            capture_output=True,
        ).stdout
    )
    for package in metadata["packages"]:
        if package["name"] == "marqdo-cli":
            return package["version"]
    fail("cargo metadata has no marqdo-cli package")


def toolchain() -> dict:
    rustc = run(["rustc", "-vV"], capture_output=True).stdout
    release = ""
    commit = ""
    for line in rustc.splitlines():
        if line.startswith("release: "):
            release = line[len("release: ") :]
        elif line.startswith("commit-hash: "):
            commit = line[len("commit-hash: ") :]
    cargo = run(["cargo", "--version"], capture_output=True).stdout.strip()
    return {"rustc": release, "rustc_commit": commit, "cargo": cargo}


def check_clean(source_sha: str) -> None:
    head = git("rev-parse", "HEAD")
    if head != source_sha:
        fail(f"HEAD {head} does not match --source-sha {source_sha}")
    status = git("status", "--porcelain")
    allowed = [
        line
        for line in status.splitlines()
        if not (line.startswith("?? dist/") or line.startswith("!! dist/"))
    ]
    if allowed:
        fail("working tree is not clean:\n" + "\n".join(allowed))


def build_examples() -> dict:
    outputs = {}
    for name in ("plugin-demo", "plugin-multi-export", "provider-demo"):
        script = ROOT / "examples" / name / "build.sh"
        run(["bash", str(script)], cwd=ROOT)
        outputs[name] = ROOT / "examples" / name / "build"
        for path in sorted(outputs[name].rglob("*")):
            if path.is_file():
                outputs.setdefault("files", {})[str(path.relative_to(ROOT))] = sha256_file(path)
    return outputs


def copy_tree(source: Path, destination: Path) -> None:
    shutil.copytree(source, destination, dirs_exist_ok=True)


def stage(stage_dir: Path, version: str, source_sha: str, target: str) -> dict:
    stage_dir.mkdir(parents=True)
    release_dir = ROOT / "target" / "release"
    for name in BINARIES:
        binary = release_dir / name
        if not binary.is_file():
            fail(f"missing release binary {binary}; build with --release --locked first")
        installed = stage_dir / name
        shutil.copy2(binary, installed)
        installed.chmod(0o755)

    for name in ("README.md", "CHANGELOG.md", "COMPATIBILITY.md"):
        shutil.copy2(ROOT / name, stage_dir / name)
    for license_name in ("LICENSE-MIT", "LICENSE-APACHE"):
        shutil.copy2(ROOT / license_name, stage_dir / license_name)

    copy_tree(ROOT / "spec", stage_dir / "spec")
    copy_tree(ROOT / "docs" / "developers", stage_dir / "docs" / "developers")

    # Examples without local build output, then the freshly built extension
    # artifacts so a clean install can run every example without a compiler.
    copy_tree(ROOT / "examples", stage_dir / "examples")
    for build_dir in stage_dir.glob("examples/*/build"):
        shutil.rmtree(build_dir, ignore_errors=True)
    for example in ("plugin-demo", "plugin-multi-export", "provider-demo"):
        copy_tree(ROOT / "examples" / example / "build", stage_dir / "examples" / example / "build")

    copy_tree(ROOT / "tests" / "consumers", stage_dir / "tests" / "consumers")

    limitations = ROOT / "docs" / "developers" / "release-limits.md"
    shutil.copy2(limitations, stage_dir / "KNOWN_LIMITATIONS.md")

    commit_time = int(git("show", "-s", "--format=%ct", "HEAD"))
    binaries = {
        name: {
            "sha256": sha256_file(stage_dir / name),
            "size": (stage_dir / name).stat().st_size,
        }
        for name in BINARIES
    }
    info = {
        "schema": "marqdo.release-info",
        "schema_version": 1,
        "product": "marqdo-cli",
        "product_version": version,
        "source_sha": source_sha,
        "target_triple": target,
        "build_profile": "release",
        "commit_time_utc": time_utc(commit_time),
        "toolchain": toolchain(),
        "binaries": binaries,
        "protocols": {
            "analysis": 2,
            "trace": 2,
            "tool": 1,
            "plugin_abi": "1.x",
            "foreign_wire": 1,
            "provider_manifest": 2,
            "lock": 4,
            "markdown_profile": "marqdo.md.v1",
        },
        "supported_platforms": ["x86_64-unknown-linux-gnu"],
        "known_limitations": "KNOWN_LIMITATIONS.md",
        "notes": [
            "Checksums prove content integrity only; they are not a publisher signature.",
            "The provider worker must stay in the same directory as marqdo.",
            "Run `python3 tools/release/smoke.py --archive <this tarball>` before publishing.",
        ],
    }
    (stage_dir / "RELEASE_INFO.json").write_text(
        json.dumps(info, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (stage_dir / "SOURCE_SHA").write_text(source_sha + "\n", encoding="utf-8")
    return {"info": info, "commit_time": commit_time}


def time_utc(epoch: int) -> str:
    import datetime

    return (
        datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def make_archive(stage_dir: Path, archive: Path, commit_time: int) -> None:
    entries = sorted(path for path in stage_dir.rglob("*") if path.is_file())
    entries.sort(key=lambda path: str(path.relative_to(stage_dir.parent)))
    with archive.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as tar:
                for path in entries:
                    arcname = str(path.relative_to(stage_dir.parent))
                    info = tar.gettarinfo(str(path), arcname=arcname)
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    info.mtime = commit_time
                    # Preserve the executable bit of scripts/artifacts; only
                    # force the two release binaries themselves to 0755.
                    if path.name in BINARIES:
                        info.mode = 0o755
                    else:
                        info.mode = path.stat().st_mode & 0o777
                    with path.open("rb") as handle:
                        tar.addfile(info, handle)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", default=None)
    parser.add_argument("--out-dir", default=str(ROOT / "dist"))
    parser.add_argument("--skip-example-build", action="store_true")
    args = parser.parse_args(argv)

    if not args.source_sha or len(args.source_sha) != 40 or any(
        ch not in "0123456789abcdef" for ch in args.source_sha
    ):
        fail("--source-sha must be a full lowercase 40-character commit SHA")
    check_clean(args.source_sha)
    version = workspace_version()
    if version != args.version:
        fail(f"workspace version is {version}, not {args.version}")
    target = args.target or host_target()
    if target != "x86_64-unknown-linux-gnu":
        fail(f"this beta release procedure supports x86_64-unknown-linux-gnu only, got {target}")
    if not args.skip_example_build:
        build_examples()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    stage_dir = out_dir / f"marqdo-{version}-{target}"
    shutil.rmtree(stage_dir, ignore_errors=True)
    staged = stage(stage_dir, version, args.source_sha, target)
    archive = out_dir / f"marqdo-{version}-{target}.tar.gz"
    archive.unlink(missing_ok=True)
    make_archive(stage_dir, archive, staged["commit_time"])
    digest = sha256_file(archive)
    checksum = out_dir / f"{archive.name}.sha256"
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "archive": str(archive),
                "sha256": digest,
                "checksum_file": str(checksum),
                "size": archive.stat().st_size,
                "source_sha": args.source_sha,
                "version": version,
                "target": target,
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
