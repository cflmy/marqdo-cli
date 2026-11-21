#!/usr/bin/env python3
"""Unit tests for the external-extension evidence validator (RFC-EXT-0001 §11.3)."""

import json
import unittest

import verify_extension_contracts as evidence


def valid_manifest() -> dict:
    return {
        "schema": "marqdo.extension-evidence",
        "schema_version": 1,
        "tested_sha": "a" * 40,
        "cli_binary_sha256": "b" * 64,
        "fixture_repo_sha": "c" * 40,
        "os": "linux",
        "arch": "x86_64",
        "results": {
            name: "PASS" for name in evidence.REQUIRED_RESULTS
        },
        "commands": ["python3 tests/run_all.py"],
        "failures": [],
        "unverified": [],
    }


class ValidateManifestTests(unittest.TestCase):
    def test_valid_manifest_has_no_problems(self) -> None:
        self.assertEqual(evidence.validate_manifest(valid_manifest()), [])

    def test_missing_result_is_rejected(self) -> None:
        manifest = valid_manifest()
        del manifest["results"]["resource_provider_cli"]
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("resource_provider_cli" in item for item in problems), problems)

    def test_fail_result_is_rejected(self) -> None:
        manifest = valid_manifest()
        manifest["results"]["plugin_worker"] = "FAIL"
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("plugin_worker" in item for item in problems), problems)

    def test_unknown_result_key_is_rejected(self) -> None:
        manifest = valid_manifest()
        manifest["results"]["surprise"] = "PASS"
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("surprise" in item for item in problems), problems)

    def test_short_sha_is_rejected(self) -> None:
        manifest = valid_manifest()
        manifest["tested_sha"] = "deadbeef"
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("tested_sha" in item for item in problems), problems)

    def test_wrong_schema_version_is_rejected(self) -> None:
        manifest = valid_manifest()
        manifest["schema_version"] = 2
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("schema_version" in item for item in problems), problems)

    def test_empty_commands_is_rejected(self) -> None:
        manifest = valid_manifest()
        manifest["commands"] = []
        problems = evidence.validate_manifest(manifest)
        self.assertTrue(any("commands" in item for item in problems), problems)

    def test_documented_schema_is_valid_json(self) -> None:
        schema_path = (
            evidence.REPO
            / "docs"
            / "evidence"
            / "extension"
            / "schema.json"
        )
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        self.assertEqual(schema["properties"]["schema"]["const"], "marqdo.extension-evidence")


if __name__ == "__main__":
    unittest.main()
