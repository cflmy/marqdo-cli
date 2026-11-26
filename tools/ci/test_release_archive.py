"""Release-archive contract tests (RFC-EXT-0003 H-08 / T-PKG-01..05).

These tests are stdlib-only and do not build an archive; they pin the digest
helpers that package.py/smoke.py share with the runtime and check that the
release pipeline takes the stdlib Bundle as one explicit, verified input
instead of relying on the compile-time source path (ADR-0070).
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import shutil
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


class ReleaseArchiveContractTest(unittest.TestCase):
    def setUp(self) -> None:
        self.package = load_module("release_package", ROOT / "tools/release/package.py")
        self.smoke = load_module("release_smoke", ROOT / "tools/release/smoke.py")
        self.manifest = json.loads((ROOT / "stdlib/stdlib.json").read_text(encoding="utf-8"))
        self.pin = json.loads((ROOT / "tools/stdlib/bundle-pin.json").read_text(encoding="utf-8"))

    def test_tree_digest_matches_manifest(self) -> None:
        self.assertEqual(
            self.package.stdlib_tree_digest(ROOT / "stdlib"), self.manifest["digest"]
        )
        self.assertEqual(
            self.smoke.stdlib_tree_digest(ROOT / "stdlib"), self.manifest["digest"]
        )

    def test_alias_digest_matches_manifest(self) -> None:
        self.assertEqual(
            self.package.stdlib_aliases_digest(self.manifest),
            self.manifest["aliases_digest"],
        )
        self.assertEqual(
            self.smoke.stdlib_aliases_digest(self.manifest),
            self.manifest["aliases_digest"],
        )

    def test_verify_record_names_required_files_and_count(self) -> None:
        record = self.package.verify_stdlib(
            ROOT / "stdlib", ROOT / "tools/stdlib/bundle-pin.json"
        )
        self.assertEqual(record["name"], self.manifest["name"])
        self.assertEqual(record["version"], self.manifest["version"])
        self.assertEqual(record["tree_digest"], self.manifest["digest"])
        self.assertEqual(record["aliases_digest"], self.manifest["aliases_digest"])
        self.assertIn("stdlib.json", record["files"])
        self.assertIn("io.mq.md", record["files"])
        self.assertIn("io/console.mq.md", record["files"])
        self.assertEqual(record["file_count"], len(record["files"]))
        self.assertEqual(record["source"]["commit"], self.pin["source"]["commit"])
        self.assertEqual(record["source"]["kind"], "in-repo")

    def test_verify_record_has_no_source_without_a_pin(self) -> None:
        record = self.package.verify_stdlib(ROOT / "stdlib")
        self.assertNotIn("source", record)

    def test_missing_explicit_input_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(SystemExit) as raised:
                    self.package.verify_stdlib(Path(tmp) / "missing")
            self.assertEqual(raised.exception.code, 1)
            self.assertIn("does not exist", stderr.getvalue())

    def test_tampered_explicit_input_and_wrong_pin_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "stdlib"
            shutil.copytree(ROOT / "stdlib", copy)
            (copy / "io.mq.md").write_text("## @knowledge tampered\n", encoding="utf-8")
            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    self.package.verify_stdlib(copy)
            self.assertEqual(raised.exception.code, 1)

            shutil.rmtree(copy)
            shutil.copytree(ROOT / "stdlib", copy)
            restamped = json.loads((copy / "stdlib.json").read_text(encoding="utf-8"))
            restamped["version"] = "9.9.9"
            (copy / "stdlib.json").write_text(json.dumps(restamped), encoding="utf-8")
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(SystemExit) as raised:
                    self.package.verify_stdlib(copy, ROOT / "tools/stdlib/bundle-pin.json")
            self.assertEqual(raised.exception.code, 1)
            self.assertIn("does not match the pin", stderr.getvalue())

    def test_package_takes_one_explicit_stdlib_dir(self) -> None:
        source = (ROOT / "tools/release/package.py").read_text(encoding="utf-8")
        self.assertIn('add_argument("--stdlib-dir", required=True)', source)
        self.assertIn('add_argument("--bundle-pin"', source)
        self.assertIn('copy_tree(stdlib_dir, stage_dir / "stdlib")', source)
        self.assertIn('"stdlib": stdlib_info', source)
        self.assertIn("verify_stdlib(", source)
        self.assertNotIn('STDLIB = ROOT / "stdlib"', source)
        self.assertIn('"math-backend"', source.replace("math_backend", "math-backend"))

    def test_archive_declares_only_current_analysis_revision(self) -> None:
        self.smoke.verify_analysis_metadata({"analysis": 3})
        for malformed in ({"analysis": 2}, {"analysis": 3, "analysis_versions": [3]}, {}):
            with self.subTest(protocols=malformed), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    self.smoke.verify_analysis_metadata(malformed)

    def test_release_discovery_does_not_fall_back_to_the_source_tree(self) -> None:
        source = (ROOT / "crates/cli/src/stdlib.rs").read_text(encoding="utf-8")
        fallback = source.index("CARGO_MANIFEST_DIR")
        self.assertIn("#[cfg(debug_assertions)]", source[:fallback])
        # The compile-time path must be inside the debug-only block: a release
        # build cannot reach it.
        debug_block = source[fallback - 200 : fallback + 400]
        self.assertIn("#[cfg(debug_assertions)]", debug_block)

    def test_smoke_verifies_and_fails_closed_without_stdlib(self) -> None:
        source = (ROOT / "tools/release/smoke.py").read_text(encoding="utf-8")
        self.assertIn("verify_stdlib_bundle(install, info)", source)
        self.assertIn("verify_stdlib_bundle", source)
        self.assertIn("stdlib.hidden", source)
        self.assertIn("expected 6", source)
        self.assertIn("permission.denied", source)


if __name__ == "__main__":
    unittest.main()
