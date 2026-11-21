import importlib.util
import unittest
from pathlib import Path

MODULE = Path(__file__).with_name("release_evidence.py")
spec = importlib.util.spec_from_file_location("release_evidence", MODULE)
release_evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_evidence)

SHA = "a" * 40
OTHER = "b" * 40


def gate(name: str, state: str, sha: str = SHA, url: str = "https://example.test/run") -> str:
    return f"{name}={state}:{sha}:{url}"


class ReleaseEvidenceTests(unittest.TestCase):
    def test_all_required_pass_is_candidate(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("windows", "PASS"),
                gate("macos", "PASS"),
                gate("strict_vm", "PASS"),
                gate("performance", "WARN"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "RELEASE_CANDIDATE")
        self.assertEqual(manifest["reasons"], [])

    def test_missing_strict_vm_is_not_release(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("windows", "PASS"),
                gate("macos", "PASS"),
                gate("performance", "PASS"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        self.assertTrue(any("strict_vm" in reason for reason in manifest["reasons"]))

    def test_sha_mismatch_is_not_release(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("windows", "PASS", OTHER),
                gate("macos", "PASS"),
                gate("strict_vm", "PASS"),
                gate("performance", "PASS"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        self.assertTrue(any("windows" in reason for reason in manifest["reasons"]))

    def test_failure_is_not_release(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "FAIL"),
                gate("windows", "PASS"),
                gate("macos", "PASS"),
                gate("strict_vm", "PASS"),
                gate("performance", "PASS"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")

    def test_unverified_performance_is_not_release(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("windows", "PASS"),
                gate("macos", "PASS"),
                gate("strict_vm", "PASS"),
                gate("performance", "UNVERIFIED"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")

    def test_beta_profile_accepts_declared_linux_scope_with_warn_performance(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("strict_vm", "PASS"),
                gate("extension", "PASS"),
                gate("clean_install", "PASS"),
                gate("version_compat", "PASS"),
                gate("windows", "UNSUPPORTED", url="linux-only-support-matrix"),
                gate("macos", "UNSUPPORTED", url="linux-only-support-matrix"),
                gate("performance", "WARN"),
            ],
            profile="beta",
        )
        self.assertEqual(manifest["release_decision"], "BETA_RELEASE_CANDIDATE")
        self.assertEqual(manifest["profile"], "beta")
        self.assertEqual(manifest["reasons"], [])

    def test_beta_profile_allows_tested_fail_closed_strict_policy(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("strict_vm", "UNSUPPORTED", url="strict.fail_closed_test"),
                gate("extension", "PASS"),
                gate("clean_install", "PASS"),
                gate("version_compat", "PASS"),
                gate("windows", "UNSUPPORTED"),
                gate("macos", "UNSUPPORTED"),
                gate("performance", "WARN"),
            ],
            profile="beta",
        )
        self.assertEqual(manifest["release_decision"], "BETA_RELEASE_CANDIDATE")

    def test_beta_profile_requires_explicit_platform_and_extension_gates(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("strict_vm", "PASS"),
                gate("clean_install", "PASS"),
                gate("version_compat", "PASS"),
                gate("performance", "WARN"),
            ],
            profile="beta",
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        joined = " ".join(manifest["reasons"])
        self.assertIn("extension: missing gate", joined)
        self.assertNotIn("windows: missing gate", joined)

    def test_beta_profile_rejects_wrong_sha_and_unverified_gates(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS", OTHER),
                gate("strict_vm", "PASS"),
                gate("extension", "UNVERIFIED"),
                gate("clean_install", "PASS"),
                gate("version_compat", "PASS"),
                gate("windows", "UNSUPPORTED"),
                gate("macos", "UNSUPPORTED"),
                gate("performance", "PASS"),
            ],
            profile="beta",
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        joined = " ".join(manifest["reasons"])
        self.assertIn("linux", joined)
        self.assertIn("extension", joined)

    def test_stable_profile_rejects_beta_unsupported_states(self) -> None:
        manifest = release_evidence.build_manifest(
            SHA,
            [
                gate("linux", "PASS"),
                gate("windows", "UNSUPPORTED"),
                gate("macos", "UNSUPPORTED"),
                gate("strict_vm", "UNSUPPORTED"),
                gate("performance", "WARN"),
            ],
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")

    def test_unknown_profile_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            release_evidence.build_manifest(SHA, [], profile="lts")

    def test_unknown_gate_and_bad_sha_are_rejected(self) -> None:
        with self.assertRaises(ValueError):
            release_evidence.build_manifest(SHA, [gate("solaris", "PASS")])
        with self.assertRaises(ValueError):
            release_evidence.build_manifest("not-a-sha", [])


if __name__ == "__main__":
    unittest.main()
