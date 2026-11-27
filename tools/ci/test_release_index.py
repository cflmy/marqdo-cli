"""Release index ties an archive and the producer receipt to authoritative CI."""

import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
import release_index


class ReleaseIndexTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.archive = Path(directory.name) / "marqdo-3.0.0-rc.6-x86_64-unknown-linux-gnu.tar.gz"
        self.sha = "9" * 40
        self.bundle = {"name": "marqdo-stdlib", "version": "0.2.0",
                       "tree_digest": "sha256:" + "1" * 64,
                       "aliases_digest": "sha256:" + "2" * 64}
        self.receipt = {**self.bundle, "root_commit": "8" * 40,
                        "components": [{"id": "std-io", "revision": "7" * 40}]}
        self.info = {
            "schema": "marqdo.release-info", "schema_version": 1, "product": "marqdo-cli",
            "build_profile": "release", "product_version": "3.0.0-rc.6",
            "target_triple": "x86_64-unknown-linux-gnu", "source_sha": self.sha,
            "stdlib": {**self.bundle, "source": {"kind": "ecosystem",
                       "repository": "cflmy/marqdo", "commit": self.receipt["root_commit"]}},
        }
        steps = [{"name": name, "type": "commands", "state": "success"}
                 for name in sorted(release_index.REQUIRED_STEPS)]
        self.pipeline = {"number": 3, "commit": self.sha, "status": "success", "event": "push",
                         "workflows": [{"name": "main", "state": "success", "children": steps}]}
        self.repo = {"id": 2, "forge_remote_id": "19", "full_name": release_index.REPOSITORY}

        with tarfile.open(self.archive, "w:gz") as output:
            top = self.archive.name.removesuffix(".tar.gz")
            for name, data in (("RELEASE_INFO.json", json.dumps(self.info)), ("SOURCE_SHA", self.sha)):
                member = tarfile.TarInfo(f"{top}/{name}")
                contents = data.encode()
                member.size = len(contents)
                output.addfile(member, io.BytesIO(contents))
        self.checksum = self.archive.with_name(self.archive.name + ".sha256")
        self.digest = release_index.sha256_file(self.archive)
        self.checksum.write_text(f"{self.digest}  {self.archive.name}\n", encoding="utf-8")

    def test_assets_and_dependencies_are_derived_from_the_inputs(self):
        with patch.object(release_index, "read_json", side_effect=[{"id": 19}, self.repo, self.pipeline]):
            index = release_index.evidence(self.archive, self.receipt, 3)
        self.assertEqual(index["ci"], {"pipeline_number": 3, "source_sha": self.sha,
                                      "url": f"{release_index.CI}/repos/2/pipeline/3"})
        self.assertEqual(index["assets"][0], {"name": self.archive.name, "sha256": self.digest,
                                             "size": self.archive.stat().st_size,
                                             "target": self.info["target_triple"]})
        self.assertEqual(index["dependencies"], [{"id": "std-io", "source_sha": "7" * 40}])

    def test_index_rejects_green_pipeline_without_exact_sha_and_gates(self):
        wrong_commit = {**self.pipeline, "commit": "0" * 40}
        skipped_gate = {**self.pipeline["workflows"][0]}
        skipped_gate["children"] = skipped_gate["children"][1:]
        for pipeline in (wrong_commit, {**self.pipeline, "workflows": [skipped_gate]}):
            with (patch.object(release_index, "read_json", side_effect=[{"id": 19}, self.repo, pipeline]),
                  self.assertRaisesRegex(ValueError, "exact SHA")):
                release_index.evidence(self.archive, self.receipt, 3)

    def test_index_rejects_mismatched_receipt_or_archive_checksum(self):
        receipt = {**self.receipt, "root_commit": "0" * 40}
        with self.assertRaisesRegex(ValueError, "mismatches the Bundle"):
            release_index.evidence(self.archive, receipt, 3)
        self.checksum.write_text(f"{'0' * 64}  {self.archive.name}\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "checksum disagrees"):
            release_index.evidence(self.archive, self.receipt, 3)


if __name__ == "__main__":
    unittest.main()
