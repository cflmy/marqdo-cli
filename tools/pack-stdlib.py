#!/usr/bin/env python3
"""Build a deterministic offline marqdo-stdlib package.

The archive is intended for the separate external stdlib repository/release
workflow, not for the CLI's in-repo development fallback. It contains the
Markdown stdlib tree, stdlib.json and README.md, with stable mtimes/ownership.
"""

import hashlib
import io
import json
import os
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STDLIB = ROOT / "stdlib"
DIST = ROOT / "dist"


def tree_digest() -> str:
    digest = hashlib.sha256()
    for path in sorted(STDLIB.rglob("*.mq.md")):
        relative = path.relative_to(STDLIB).as_posix()
        digest.update(relative.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def main() -> int:
    manifest = json.loads((STDLIB / "stdlib.json").read_text())
    version = manifest["version"]
    digest = tree_digest()
    if manifest["digest"] != f"sha256:{digest}":
        raise SystemExit("stdlib.json digest does not match the tree")

    files = sorted(
        [p for p in STDLIB.rglob("*") if p.is_file() and p.name != "stdlib.json"]
        + [STDLIB / "stdlib.json"],
        key=lambda p: p.relative_to(STDLIB).as_posix(),
    )
    DIST.mkdir(exist_ok=True)
    archive = DIST / f"marqdo-stdlib-{version}.tar.gz"
    with archive.open("wb") as raw:
        with gzip_open(raw) as gz:
            with tarfile.open(fileobj=gz, mode="w") as tar:
                for path in files:
                    relative = path.relative_to(STDLIB).as_posix()
                    info = tarfile.TarInfo(name=relative)
                    data = path.read_bytes()
                    info.size = len(data)
                    info.mode = 0o644
                    info.uid = info.gid = 0
                    info.uname = info.gname = "root"
                    info.mtime = 0
                    tar.addfile(info, io.BytesIO(data))
    archive_sha = hashlib.sha256(archive.read_bytes()).hexdigest()
    descriptor = {
        "schema": "marqdo.stdlib-package",
        "schema_version": 1,
        "name": manifest["name"],
        "version": version,
        "tree_digest": f"sha256:{digest}",
        "archive": archive.name,
        "archive_sha256": f"sha256:{archive_sha}",
        "files": [p.relative_to(STDLIB).as_posix() for p in files],
    }
    descriptor_path = DIST / f"marqdo-stdlib-{version}.json"
    descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")
    (DIST / f"{archive.name}.sha256").write_text(
        f"{archive_sha}  {archive.name}\n"
    )
    print(json.dumps(descriptor, indent=2))
    return 0


def gzip_open(raw):
    import gzip

    return gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="")


if __name__ == "__main__":
    raise SystemExit(main())
