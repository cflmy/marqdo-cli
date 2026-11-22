"""Release-archive contract tests (RFC-EXT-0003 H-08 / T-PKG-01..05).

These tests are stdlib-only and do not build an archive; they pin the digest
helpers that package.py/smoke.py share with the runtime and check that the
release pipeline references a verified stdlib bundle instead of relying on the
compile-time source path.
"""

from __future__ import annotations

import importlib.util
import json
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

    def test_tree_digest_matches_manifest(self) -> None:
        self.assertEqual(
            self.package.stdlib_tree_digest(), self.manifest["digest"]
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
        record = self.package.verify_stdlib()
        self.assertEqual(record["name"], self.manifest["name"])
        self.assertEqual(record["version"], self.manifest["version"])
        self.assertEqual(record["tree_digest"], self.manifest["digest"])
        self.assertEqual(record["aliases_digest"], self.manifest["aliases_digest"])
        self.assertIn("stdlib.json", record["files"])
        self.assertIn("io.mq.md", record["files"])
        self.assertIn("io/console.mq.md", record["files"])
        self.assertEqual(record["file_count"], len(record["files"]))

    def test_package_stages_stdlib_next_to_the_binary_and_math_backend(self) -> None:
        source = (ROOT / "tools/release/package.py").read_text(encoding="utf-8")
        self.assertIn('copy_tree(STDLIB, stage_dir / "stdlib")', source)
        self.assertIn('"math-backend"', source.replace("math_backend", "math-backend"))
        self.assertIn('"stdlib": stdlib_info', source)
        self.assertIn("verify_stdlib()", source)

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
