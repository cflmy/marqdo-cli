#!/usr/bin/env python3
"""Build a deterministic offline marqdo-stdlib package from an explicit Bundle.

The input is one unpacked Bundle root (a directory that contains
`stdlib.json`), passed with the required `--stdlib-dir`. This script does not
read the checkout's `stdlib/` implicitly: the CLI release and the separate
stdlib release workflow both name the Bundle they package (ADR-0070).

The archive contains the Markdown stdlib tree, `stdlib.json` and every other
file in the Bundle, with stable mtimes/ownership. An optional `--bundle-pin`
also requires the input to match the pinned name/version/commit/digests.

Usage:

    python3 tools/pack-stdlib.py --stdlib-dir stdlib \
        --bundle-pin tools/stdlib/bundle-pin.json
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"

sys.path.insert(0, str(Path(__file__).resolve().parent / "stdlib"))
import bundle as stdlib_bundle  # noqa: E402


def gzip_open(raw):
    import gzip

    return gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="")


def verify_input(root: Path, pin_path: Path | None) -> dict:
    try:
        record = stdlib_bundle.verify(root)
        stdlib_bundle.require_files(record, ("stdlib.json",))
        if record["aliases_digest"] is None:
            raise stdlib_bundle.BundleError(
                f"{root / 'stdlib.json'} declares no aliases_digest"
            )
        if pin_path is not None:
            pin = stdlib_bundle.load_pin(pin_path)
            stdlib_bundle.verify_pin(record, pin)
    except stdlib_bundle.BundleError as error:
        raise SystemExit(f"pack-stdlib: error: {error}") from error
    return record


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stdlib-dir", required=True)
    parser.add_argument("--out-dir", default=str(DIST))
    parser.add_argument("--bundle-pin", default=None)
    args = parser.parse_args(argv)

    root = Path(args.stdlib_dir)
    record = verify_input(root, Path(args.bundle_pin) if args.bundle_pin else None)
    version = record["version"]
    files = [root / name for name in record["files"]]
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    archive = out_dir / f"marqdo-stdlib-{version}.tar.gz"
    with archive.open("wb") as raw:
        with gzip_open(raw) as gz:
            with tarfile.open(fileobj=gz, mode="w") as tar:
                for path in files:
                    relative = path.relative_to(root).as_posix()
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
        "name": record["name"],
        "version": version,
        "tree_digest": record["tree_digest"],
        "aliases_digest": record["aliases_digest"],
        "archive": archive.name,
        "archive_sha256": f"sha256:{archive_sha}",
        "files": record["files"],
    }
    descriptor_path = out_dir / f"marqdo-stdlib-{version}.json"
    descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")
    (out_dir / f"{archive.name}.sha256").write_text(f"{archive_sha}  {archive.name}\n")
    print(json.dumps(descriptor, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
