"""Explicit stdlib Bundle input tests (ADR-0070 / issue #11 Phase A).

Covers the shared resolver (`tools/stdlib/bundle.py`) and the deterministic
package entry point (`tools/pack-stdlib.py`). The digest helpers here are
independent re-implementations so a bug in the shared helper cannot make the
test agree with itself.
"""

from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import os
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def independent_tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    paths = sorted(
        (path.relative_to(root).as_posix(), path)
        for path in root.rglob("*.mq.md")
        if path.is_file()
    )
    for relative, path in paths:
        digest.update(relative.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return "sha256:" + digest.hexdigest()


def independent_aliases_digest(aliases: list[dict]) -> str:
    digest = hashlib.sha256()
    for alias in sorted(aliases, key=lambda item: (item["target_module"], item["target_function"])):
        digest.update(alias["target_module"].encode())
        digest.update(b"\0")
        digest.update(alias["target_function"].encode())
        digest.update(b"\0")
        for name in sorted(alias["public_names"]):
            digest.update(name.encode())
            digest.update(b"\0")
    return "sha256:" + digest.hexdigest()


ALIASES = [
    {
        "public_names": ["print", "打印"],
        "target_module": "io.mq.md",
        "target_function": "写值",
        "expected_signature": "params(值)",
        "effects": ["write"],
    }
]


def make_bundle(root: Path, *, version: str = "9.9.9", name: str = "marqdo-stdlib") -> dict:
    """Create a minimal Bundle with independently computed digests."""
    root.mkdir(parents=True)
    (root / "README.md").write_text("# test bundle\n")
    (root / "io.mq.md").write_text("## @knowledge io\n")
    (root / "io").mkdir()
    (root / "io" / "console.mq.md").write_text("## @knowledge console\n")
    manifest = {
        "schema": "marqdo.stdlib-bundle",
        "schema_version": 1,
        "name": name,
        "version": version,
        "digest": independent_tree_digest(root),
        "aliases_digest": independent_aliases_digest(ALIASES),
        "public_aliases": ALIASES,
    }
    (root / "stdlib.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def write_pin(path: Path, manifest: dict, source_path: str, commit: str = "a" * 40) -> dict:
    pin = {
        "schema": "marqdo.stdlib-bundle-pin",
        "schema_version": 1,
        "name": manifest["name"],
        "version": manifest["version"],
        "source": {"kind": "test", "repository": "test/test", "commit": commit, "path": source_path},
        "tree_digest": manifest["digest"],
        "aliases_digest": manifest["aliases_digest"],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(pin, indent=2) + "\n")
    return pin


class BundleResolverTest(unittest.TestCase):
    def setUp(self) -> None:
        self.bundle = load_module("stdlib_bundle", ROOT / "tools/stdlib/bundle.py")
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)

    def test_accepted_ecosystem_pin_requires_an_explicit_bundle(self) -> None:
        pin = self.bundle.load_pin()
        self.assertEqual(pin["source"]["kind"], "ecosystem")
        self.assertNotIn("path", pin["source"])
        with self.assertRaisesRegex(
            self.bundle.BundleError, "not vendored in this repository"
        ):
            self.bundle.resolve(None, ROOT / "tools/stdlib/bundle-pin.json")

    def test_explicit_valid_bundle_still_has_to_match_the_pin(self) -> None:
        other = self.dir / "other-bundle"
        matching = make_bundle(other)
        pin_path = self.dir / "pin.json"
        write_pin(pin_path, matching, str(other))
        root, record, _ = self.bundle.resolve(other, pin_path)
        self.assertEqual(root, other.resolve())
        self.assertEqual(record["name"], "marqdo-stdlib")

        pinned = load_module("stdlib_bundle_pin_check", ROOT / "tools/stdlib/bundle.py")
        self.assertRaisesRegex(
            pinned.BundleError,
            "does not match the pin",
            pinned.verify_pin,
            {"name": "marqdo-stdlib", "version": "0.1.0",
             "tree_digest": "sha256:" + "0" * 64, "aliases_digest": "sha256:" + "0" * 64},
            pinned.load_pin(ROOT / "tools/stdlib/bundle-pin.json"),
        )

    def test_bad_explicit_input_fails_even_when_the_default_is_valid(self) -> None:
        with self.assertRaisesRegex(self.bundle.BundleError, "does not exist"):
            self.bundle.resolve(self.dir / "missing")

    def test_missing_manifest_fails(self) -> None:
        bare = self.dir / "bare"
        bare.mkdir()
        (bare / "io.mq.md").write_text("## @knowledge io\n")
        with self.assertRaisesRegex(self.bundle.BundleError, "has no manifest"):
            self.bundle.verify(bare)

    def test_tree_digest_mismatch_fails(self) -> None:
        bundle = self.dir / "tampered"
        make_bundle(bundle)
        (bundle / "io.mq.md").write_text("## @knowledge tampered\n")
        with self.assertRaisesRegex(self.bundle.BundleError, "tree digest mismatch"):
            self.bundle.verify(bundle)

    def test_alias_digest_mismatch_fails(self) -> None:
        bundle = self.dir / "alias-tampered"
        manifest = make_bundle(bundle)
        manifest["aliases_digest"] = "sha256:" + "0" * 64
        (bundle / "stdlib.json").write_text(json.dumps(manifest) + "\n")
        with self.assertRaisesRegex(self.bundle.BundleError, "alias digest mismatch"):
            self.bundle.verify(bundle)

    def test_public_aliases_without_a_digest_fail(self) -> None:
        bundle = self.dir / "no-alias-digest"
        manifest = make_bundle(bundle)
        del manifest["aliases_digest"]
        (bundle / "stdlib.json").write_text(json.dumps(manifest) + "\n")
        with self.assertRaisesRegex(self.bundle.BundleError, "without aliases_digest"):
            self.bundle.verify(bundle)

    def test_symlink_inside_the_bundle_is_rejected(self) -> None:
        bundle = self.dir / "linked"
        make_bundle(bundle)
        try:
            (bundle / "escape.mq.md").symlink_to("/etc/hostname")
        except (OSError, NotImplementedError):
            self.skipTest("symbolic links are unavailable on this host")
        with self.assertRaisesRegex(self.bundle.BundleError, "symbolic link"):
            self.bundle.verify(bundle)

    def test_bad_pin_schema_is_rejected(self) -> None:
        pin_path = self.dir / "bad-pin.json"
        pin_path.write_text(json.dumps({"schema": "other", "schema_version": 1}) + "\n")
        with self.assertRaisesRegex(self.bundle.BundleError, "schema_version"):
            self.bundle.load_pin(pin_path)

    def test_cli_prints_the_resolved_explicit_path(self) -> None:
        bundle = self.dir / "explicit"
        manifest = make_bundle(bundle)
        pin_path = self.dir / "explicit-pin.json"
        write_pin(pin_path, manifest, str(bundle))
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            self.assertEqual(
                self.bundle.main(
                    [
                        "--stdlib-dir",
                        str(bundle),
                        "--pin",
                        str(pin_path),
                        "--print-path",
                    ]
                ),
                0,
            )
        self.assertEqual(Path(stdout.getvalue().strip()), bundle.resolve())

    def test_cli_fails_for_an_invalid_explicit_input(self) -> None:
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            self.assertEqual(self.bundle.main(["--stdlib-dir", str(self.dir / "missing")]), 1)
        self.assertIn("does not exist", stderr.getvalue())


class PackStdlibTest(unittest.TestCase):
    def setUp(self) -> None:
        self.pack = load_module("pack_stdlib", ROOT / "tools/pack-stdlib.py")
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)

    def run_pack(self, *args: str) -> int:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            return self.pack.main(list(args))

    def test_stdlib_dir_is_required(self) -> None:
        with self.assertRaises(SystemExit) as raised, contextlib.redirect_stderr(io.StringIO()):
            self.pack.main([])
        self.assertEqual(raised.exception.code, 2)

    def test_missing_input_fails(self) -> None:
        with self.assertRaises(SystemExit) as raised:
            self.run_pack("--stdlib-dir", str(self.dir / "missing"), "--out-dir", str(self.dir))
        self.assertIn("does not exist", str(raised.exception.code))

    def test_missing_manifest_fails(self) -> None:
        bare = self.dir / "bare"
        bare.mkdir()
        with self.assertRaises(SystemExit) as raised:
            self.run_pack("--stdlib-dir", str(bare), "--out-dir", str(self.dir))
        self.assertIn("has no manifest", str(raised.exception.code))

    def test_tampered_tree_fails(self) -> None:
        bundle = self.dir / "bundle"
        make_bundle(bundle)
        (bundle / "io.mq.md").write_text("## @knowledge tampered\n")
        with self.assertRaises(SystemExit) as raised:
            self.run_pack("--stdlib-dir", str(bundle), "--out-dir", str(self.dir))
        self.assertIn("tree digest mismatch", str(raised.exception.code))

    def test_pin_mismatch_fails(self) -> None:
        bundle = self.dir / "bundle"
        manifest = make_bundle(bundle)
        pin_path = self.dir / "pin.json"
        pin = write_pin(pin_path, manifest, str(bundle))
        pin["tree_digest"] = "sha256:" + "0" * 64
        pin_path.write_text(json.dumps(pin) + "\n")
        with self.assertRaises(SystemExit) as raised:
            self.run_pack(
                "--stdlib-dir", str(bundle), "--out-dir", str(self.dir), "--bundle-pin", str(pin_path)
            )
        self.assertIn("does not match the pin", str(raised.exception.code))

    def test_pack_is_deterministic_and_records_the_bundle_identity(self) -> None:
        bundle = self.dir / "bundle"
        manifest = make_bundle(bundle)
        pin_path = self.dir / "pin.json"
        write_pin(pin_path, manifest, str(bundle))
        first = self.dir / "first"
        second = self.dir / "second"
        for out in (first, second):
            self.assertEqual(
                self.run_pack(
                    "--stdlib-dir", str(bundle), "--out-dir", str(out),
                    "--bundle-pin", str(pin_path),
                ),
                0,
            )
        archive_name = "marqdo-stdlib-9.9.9.tar.gz"
        first_bytes = (first / archive_name).read_bytes()
        self.assertEqual(first_bytes, (second / archive_name).read_bytes())
        self.assertEqual(
            (first / f"{archive_name}.sha256").read_bytes(),
            (second / f"{archive_name}.sha256").read_bytes(),
        )
        descriptor = json.loads((first / "marqdo-stdlib-9.9.9.json").read_text())
        self.assertEqual(descriptor["schema"], "marqdo.stdlib-package")
        self.assertEqual(descriptor["schema_version"], 1)
        self.assertEqual(descriptor["name"], manifest["name"])
        self.assertEqual(descriptor["version"], manifest["version"])
        self.assertEqual(descriptor["tree_digest"], independent_tree_digest(bundle))
        self.assertEqual(descriptor["aliases_digest"], independent_aliases_digest(ALIASES))
        self.assertEqual(
            descriptor["archive_sha256"], "sha256:" + hashlib.sha256(first_bytes).hexdigest()
        )
        self.assertEqual(descriptor["files"], sorted(descriptor["files"]))

        with tarfile.open(first / archive_name, mode="r:gz") as archive:
            members = archive.getmembers()
            self.assertEqual([m.name for m in members], descriptor["files"])
            for member in members:
                self.assertEqual(member.mtime, 0)
                self.assertIn(member.uid, (0,))
                self.assertEqual(member.gid, 0)
                self.assertEqual(member.mode, 0o644)
            self.assertEqual(
                archive.extractfile("io.mq.md").read(), (bundle / "io.mq.md").read_bytes()
            )

    def test_explicit_bundle_packs_against_a_matching_pin(self) -> None:
        bundle = self.dir / "pack-bundle"
        manifest = make_bundle(bundle)
        pin_path = self.dir / "pack-pin.json"
        write_pin(pin_path, manifest, str(bundle))
        out = self.dir / "out"
        self.assertEqual(
            self.run_pack(
                "--stdlib-dir", str(bundle), "--out-dir", str(out),
                "--bundle-pin", str(pin_path),
            ),
            0,
        )
        descriptor = json.loads(
            (out / f"marqdo-stdlib-{manifest['version']}.json").read_text()
        )
        self.assertEqual(descriptor["tree_digest"], manifest["digest"])
        self.assertEqual(descriptor["aliases_digest"], manifest["aliases_digest"])


if __name__ == "__main__":
    unittest.main()
