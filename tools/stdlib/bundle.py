#!/usr/bin/env python3
"""One resolver and verifier for a stdlib Bundle (ADR-0070).

Every source-side consumer that needs the official Bundle — packaging tools,
product tests, the performance runner and release CI — resolves it through
this module instead of concatenating a checkout path. The identity comes from
`tools/stdlib/bundle-pin.json` (name, version, source commit, tree digest,
alias digest); an explicit input may change the location but must still match
the pinned digests.

The digest algorithms are byte-identical to `marqdo_cli::stdlib` and to
`tools/release/smoke.py` (which stays an independent verifier of the shipped
archive). Nothing here is used by the running CLI.

Usage:

    python3 tools/stdlib/bundle.py [--stdlib-dir PATH] [--pin PATH] [--print-path]

Exit status is 1 for an invalid Bundle and 2 for a bad command line.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PIN_PATH = ROOT / "tools" / "stdlib" / "bundle-pin.json"
PIN_SCHEMA = "marqdo.stdlib-bundle-pin"
PIN_SCHEMA_VERSION = 1
REQUIRED_MANIFEST_FIELDS = ("name", "version", "digest")


class BundleError(Exception):
    """An input is not a verifiable stdlib Bundle."""


def _sha256_bytes(*chunks: bytes) -> str:
    digest = hashlib.sha256()
    for chunk in chunks:
        digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def _walk_files(root: Path) -> list[Path]:
    """Every regular file under `root`, sorted, rejecting symlinks.

    Deterministic packaging must not follow a link outside the Bundle root or
    archive a link whose target differs by platform.
    """
    files: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        dirnames.sort()
        base = Path(dirpath)
        for name in dirnames:
            if (base / name).is_symlink():
                raise BundleError(
                    f"stdlib Bundle contains a symbolic link directory: "
                    f"{(base / name).relative_to(root).as_posix()}"
                )
        for name in sorted(filenames):
            path = base / name
            if path.is_symlink():
                raise BundleError(
                    f"stdlib Bundle contains a symbolic link: "
                    f"{path.relative_to(root).as_posix()}"
                )
            if path.is_file():
                files.append(path)
    files.sort(key=lambda path: path.relative_to(root).as_posix())
    return files


def load_manifest(root: Path) -> dict:
    manifest_path = root / "stdlib.json"
    if not root.is_dir():
        raise BundleError(f"stdlib Bundle directory does not exist: {root}")
    if not manifest_path.is_file():
        raise BundleError(f"stdlib Bundle has no manifest: {manifest_path}")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BundleError(f"cannot read valid JSON from {manifest_path}: {error}") from error
    if not isinstance(manifest, dict):
        raise BundleError(f"{manifest_path} must contain a JSON object")
    for field in REQUIRED_MANIFEST_FIELDS:
        value = manifest.get(field)
        if not isinstance(value, str) or not value:
            raise BundleError(f"{manifest_path} has no non-empty `{field}`")
    return manifest


def tree_digest(root: Path) -> str:
    """`relative POSIX path + NUL + bytes + NUL` over every `*.mq.md` file."""
    root = Path(root)
    chunks: list[bytes] = []
    for path in _walk_files(root):
        if not path.name.endswith(".mq.md"):
            continue
        chunks.append(path.relative_to(root).as_posix().encode())
        chunks.append(b"\0")
        chunks.append(path.read_bytes())
        chunks.append(b"\0")
    return _sha256_bytes(*chunks)


def aliases_digest(manifest: dict) -> str:
    """Canonical digest over `public_aliases`, byte-identical to the loader."""
    chunks: list[bytes] = []
    aliases = sorted(
        manifest.get("public_aliases", []),
        key=lambda alias: (alias["target_module"], alias["target_function"]),
    )
    for alias in aliases:
        chunks.append(alias["target_module"].encode())
        chunks.append(b"\0")
        chunks.append(alias["target_function"].encode())
        chunks.append(b"\0")
        for name in sorted(alias["public_names"]):
            chunks.append(name.encode())
            chunks.append(b"\0")
    return _sha256_bytes(*chunks)


def _verify_aliases(manifest: dict, manifest_path: Path) -> list[dict]:
    raw = manifest.get("public_aliases", [])
    if not isinstance(raw, list):
        raise BundleError(f"{manifest_path} public_aliases must be an array")
    seen: set[str] = set()
    aliases: list[dict] = []
    for entry in raw:
        if not isinstance(entry, dict):
            raise BundleError(f"{manifest_path} public alias must be an object")
        names = entry.get("public_names")
        if not isinstance(names, list) or not names:
            raise BundleError(f"{manifest_path} public alias declares no names")
        for name in names:
            if not isinstance(name, str) or not name:
                raise BundleError(f"{manifest_path} public alias name must be a non-empty string")
            if name in seen:
                raise BundleError(f"{manifest_path} public alias name `{name}` is declared twice")
            seen.add(name)
        module = entry.get("target_module")
        if not isinstance(module, str) or not module:
            raise BundleError(f"{manifest_path} public alias has no target_module")
        relative = Path(module)
        if relative.is_absolute() or ".." in relative.parts:
            raise BundleError(
                f"{manifest_path} public alias target_module `{module}` must stay inside the Bundle"
            )
        function = entry.get("target_function")
        if not isinstance(function, str) or not function:
            raise BundleError(f"{manifest_path} public alias has no target_function")
        aliases.append(entry)
    aliases.sort(key=lambda alias: (alias["target_module"], alias["target_function"]))
    return aliases


def verify(root: Path) -> dict:
    """Validate one unpacked Bundle root and return its identity record."""
    root = Path(root)
    manifest = load_manifest(root)
    manifest_path = root / "stdlib.json"
    tree = tree_digest(root)
    declared_tree = manifest["digest"]
    if declared_tree != tree:
        raise BundleError(
            f"stdlib tree digest mismatch: stdlib.json declares {declared_tree}, "
            f"the files are {tree}"
        )
    aliases = _verify_aliases(manifest, manifest_path)
    declared_aliases = manifest.get("aliases_digest")
    actual_aliases = aliases_digest(manifest)
    if declared_aliases is None and aliases:
        raise BundleError(f"{manifest_path} declares public_aliases without aliases_digest")
    if declared_aliases is not None and declared_aliases != actual_aliases:
        raise BundleError(
            f"stdlib public-alias digest mismatch: stdlib.json declares {declared_aliases}, "
            f"the alias table is {actual_aliases}"
        )
    files = sorted(path.relative_to(root).as_posix() for path in _walk_files(root))
    modules = [name for name in files if name.endswith(".mq.md")]
    if not modules:
        raise BundleError(f"stdlib Bundle {root} contains no .mq.md module")
    return {
        "name": manifest["name"],
        "version": manifest["version"],
        "tree_digest": tree,
        "aliases_digest": actual_aliases if declared_aliases is not None else None,
        "file_count": len(files),
        "files": files,
    }


def require_files(record: dict, required: tuple[str, ...]) -> None:
    missing = [name for name in required if name not in record["files"]]
    if missing:
        raise BundleError(
            "stdlib Bundle is missing required file(s): " + ", ".join(missing)
        )


def load_pin(path: Path = PIN_PATH) -> dict:
    path = Path(path)
    try:
        pin = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BundleError(f"cannot read Bundle pin {path}: {error}") from error
    if not isinstance(pin, dict):
        raise BundleError(f"Bundle pin {path} must contain a JSON object")
    if pin.get("schema") != PIN_SCHEMA or pin.get("schema_version") != PIN_SCHEMA_VERSION:
        raise BundleError(
            f"Bundle pin {path} must be {PIN_SCHEMA} schema_version {PIN_SCHEMA_VERSION}"
        )
    source = pin.get("source")
    if not isinstance(source, dict) or not isinstance(source.get("path"), str):
        raise BundleError(f"Bundle pin {path} has no source.path")
    for field in ("name", "version", "tree_digest", "aliases_digest"):
        if not isinstance(pin.get(field), str) or not pin[field]:
            raise BundleError(f"Bundle pin {path} has no non-empty `{field}`")
    return pin


def verify_pin(record: dict, pin: dict) -> None:
    """Require a verified Bundle record to match the pinned identity."""
    for field in ("name", "version", "tree_digest", "aliases_digest"):
        if record.get(field) != pin.get(field):
            raise BundleError(
                f"stdlib Bundle does not match the pin: {field} is "
                f"{record.get(field)!r}, the pin records {pin.get(field)!r}"
            )


def resolve(
    explicit: str | Path | None = None,
    pin_path: Path = PIN_PATH,
    *,
    require_pin: bool = True,
) -> tuple[Path, dict, dict]:
    """Resolve and verify the Bundle, returning `(root, record, pin)`.

    `explicit` overrides the pinned location but never the pinned identity
    unless `require_pin` is false. A bad explicit input fails even when the
    default location is valid; there is no fallback.
    """
    pin = load_pin(pin_path)
    if explicit is None:
        root = ROOT / pin["source"]["path"]
    else:
        root = Path(explicit)
    record = verify(root)
    if require_pin:
        verify_pin(record, pin)
    require_files(
        record,
        (
            "stdlib.json",
            "io.mq.md",
            "io/console.mq.md",
        ),
    )
    return root.resolve(), record, pin


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Resolve and verify the pinned stdlib Bundle (ADR-0070)."
    )
    parser.add_argument("--stdlib-dir", default=None)
    parser.add_argument("--pin", default=str(PIN_PATH))
    parser.add_argument("--print-path", action="store_true")
    parser.add_argument("--dump-pin", action="store_true")
    args = parser.parse_args(argv)
    try:
        root, record, pin = resolve(args.stdlib_dir, Path(args.pin))
    except BundleError as error:
        print(f"bundle: error: {error}", file=sys.stderr)
        return 1
    if args.print_path:
        print(root)
        return 0
    if args.dump_pin:
        print(json.dumps(pin, indent=2, sort_keys=True))
        return 0
    print(
        json.dumps(
            {
                "root": str(root),
                "name": record["name"],
                "version": record["version"],
                "tree_digest": record["tree_digest"],
                "aliases_digest": record["aliases_digest"],
                "source_commit": pin["source"].get("commit"),
                "file_count": record["file_count"],
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
