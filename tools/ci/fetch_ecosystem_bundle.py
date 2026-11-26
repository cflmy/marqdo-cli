#!/usr/bin/env python3
"""Materialize the pinned ecosystem stdlib Bundle for CI and local verification.

Phase B removes the standard-library sources from this repository. The one
accepted Bundle identity lives in `tools/stdlib/bundle-pin.json` and names the
ecosystem root commit that pinned the component gitlinks. This helper fetches
exactly that root commit and the stdlib component gitlinks it records, runs the
root-owned producer from the pinned commit, and verifies the produced Bundle
against the pin (name, version, tree digest, alias digest).

Nothing is resolved from a branch: the pin commit and every gitlink are fixed.
The output path is printed on success; a mismatch, missing repository or missing
module fails closed. The result is a build artifact under `target/`, never a
source checkout that upstreams into this repository.

Usage:

    python3 tools/ci/fetch_ecosystem_bundle.py [--out target/ecosystem-stdlib]

Environment overrides are intentionally absent: CI and maintainers edit the pin.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PIN = ROOT / "tools" / "stdlib" / "bundle-pin.json"
FORGE = "https://git.cflmy.cn"
COMPONENTS = (
    "stdlib/collections",
    "stdlib/data",
    "stdlib/fs",
    "stdlib/io",
    "stdlib/system",
    "stdlib/text",
)


class FetchError(Exception):
    pass


def run(command, cwd=None):
    result = subprocess.run(command, cwd=cwd, capture_output=True)
    if result.returncode != 0:
        raise FetchError(
            f"command failed ({result.returncode}): {' '.join(map(str, command))}\n"
            f"{result.stderr.decode(errors='replace').strip()}"
        )
    return result.stdout.decode(errors="replace")


def load_pin() -> dict:
    try:
        pin = json.loads(PIN.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise FetchError(f"cannot read Bundle pin {PIN}: {error}") from error
    source = pin.get("source")
    if not isinstance(source, dict) or source.get("kind") != "ecosystem":
        raise FetchError(
            f"{PIN} does not name an ecosystem Bundle source; Phase B requires "
            "source.kind = 'ecosystem'"
        )
    for field in ("repository", "commit"):
        if not isinstance(source.get(field), str) or not source[field]:
            raise FetchError(f"{PIN} has no source.{field}")
    for field in ("name", "version", "tree_digest", "aliases_digest"):
        if not isinstance(pin.get(field), str) or not pin[field]:
            raise FetchError(f"{PIN} has no non-empty `{field}`")
    return pin


def fetch_root(pin: dict, cache: Path) -> Path:
    source = pin["source"]
    url = f"{FORGE}/{source['repository']}.git"
    if not cache.is_dir():
        run(["git", "init", "-q", str(cache)])
        run(["git", "-C", str(cache), "remote", "add", "origin", url])
    else:
        run(["git", "-C", str(cache), "remote", "set-url", "origin", url])
    commit = source["commit"]
    try:
        run(["git", "-C", str(cache), "cat-file", "-e", f"{commit}^{{commit}}"])
    except FetchError:
        run(["git", "-C", str(cache), "fetch", "--no-tags", "origin", commit])
    run(["git", "-C", str(cache), "checkout", "-q", "--detach", commit])
    for component in COMPONENTS:
        run(
            [
                "git",
                "-C",
                str(cache),
                "submodule",
                "update",
                "--init",
                "--depth",
                "1",
                "--",
                component,
            ]
        )
    return cache


def build_bundle(root: Path, out: Path) -> Path:
    """Build the Bundle; an existing output is reused only after verification.

    CI jobs share one workspace, so a parallel or earlier job may already have
    materialized the same pinned Bundle. The output is built in a temporary
    sibling and moved into place atomically; a mismatch is rebuilt, never used.
    """
    temp = out.with_name(out.name + ".partial-" + str(os.getpid()))
    if temp.exists():
        shutil.rmtree(temp)
    run(
        [
            sys.executable,
            str(root / "scripts" / "build-stdlib.py"),
            "--out",
            str(temp),
        ],
        cwd=root,
    )
    if out.exists():
        shutil.rmtree(out)
    temp.rename(out)
    return out


def verify(pin: dict, bundle: Path) -> dict:
    result = run(
        [
            sys.executable,
            str(ROOT / "tools" / "stdlib" / "bundle.py"),
            "--stdlib-dir",
            str(bundle),
            "--dump-pin",
        ]
    )
    pinned = json.loads(result)
    for field in ("name", "version", "tree_digest", "aliases_digest"):
        if pinned[field] != pin[field]:
            raise FetchError(
                f"produced Bundle does not match the pin field {field}: "
                f"{pinned[field]!r} != {pin[field]!r}"
            )
    return pinned


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=str(ROOT / "target" / "ecosystem-stdlib"))
    parser.add_argument("--cache", default=str(ROOT / "target" / "ecosystem-root"))
    args = parser.parse_args(argv)
    out = Path(args.out).resolve()
    cache = Path(args.cache).resolve()
    try:
        pin = load_pin()
        if out.is_dir():
            try:
                verify(pin, out)
            except (FetchError, OSError, json.JSONDecodeError):
                pass
            else:
                print(out)
                return 0
        root = fetch_root(pin, cache)
        bundle = build_bundle(root, out)
        verify(pin, bundle)
    except FetchError as error:
        print(f"fetch-ecosystem-bundle: error: {error}", file=sys.stderr)
        return 1
    print(bundle)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
