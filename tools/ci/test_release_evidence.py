import importlib.util
import io
import json
import unittest
from contextlib import redirect_stdout
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
            SHA, [gate(name, "PASS") for name in release_evidence.GATES]
        )
        self.assertEqual(manifest["release_decision"], "RELEASE_CANDIDATE")
        self.assertEqual(manifest["reasons"], [])

    def test_complete_linux_rc_is_candidate(self) -> None:
        manifest = release_evidence.build_manifest(SHA, self.rc_gates(), profile="rc")
        self.assertEqual(manifest["release_decision"], "RC_RELEASE_CANDIDATE")
        self.assertEqual(manifest["reasons"], [])

    @staticmethod
    def rc_gates() -> list[str]:
        return [
            gate(name, "UNSUPPORTED" if name in {"windows", "macos"} else "PASS")
            for name in release_evidence.GATES
        ]

    def test_rc_requires_same_sha_evidence_for_every_gate(self) -> None:
        for original in self.rc_gates():
            name, details = original.split("=", 1)
            state = details.split(":", 1)[0]
            for replacement, reason in [
                ([], "missing gate"),
                ([gate(name, state, OTHER)], "!= target_sha"),
                ([gate(name, state, url="")], "missing evidence reference"),
                ([gate(name, "FAIL")], "FAIL"),
                ([gate(name, "UNVERIFIED")], "UNVERIFIED"),
            ]:
                with self.subTest(gate=name, reason=reason):
                    inputs = [spec for spec in self.rc_gates() if spec != original]
                    manifest = release_evidence.build_manifest(
                        SHA, inputs + replacement, profile="rc"
                    )
                    self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
                    self.assertEqual(len(manifest["reasons"]), 1)
                    self.assertIn(reason, manifest["reasons"][0])

    def test_rc_rejects_beta_exceptions_and_platform_scope_changes(self) -> None:
        for name, state in [
            ("strict_vm", "UNSUPPORTED"),
            ("performance", "WARN"),
            ("windows", "PASS"),
            ("macos", "PASS"),
        ]:
            with self.subTest(gate=name, state=state):
                inputs = [spec for spec in self.rc_gates() if not spec.startswith(f"{name}=")]
                manifest = release_evidence.build_manifest(
                    SHA, inputs + [gate(name, state)], profile="rc"
                )
                self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
                self.assertTrue(manifest["reasons"][0].startswith(f"{name}:"))

    def test_rc_shared_performance_requires_explicit_approval_and_evidence(self) -> None:
        approval = "release notes: maintainer approved the shared node for this RC"
        inputs = [spec for spec in self.rc_gates() if not spec.startswith("performance=")]
        inputs.append(gate("performance", "WARN"))
        manifest = release_evidence.build_manifest(SHA, inputs, "rc", approval)
        self.assertEqual(manifest["release_decision"], "RC_RELEASE_CANDIDATE")
        self.assertEqual(manifest["gates"]["performance"]["rc_shared_node_approval"], approval)
        manifest = release_evidence.build_manifest(
            SHA, inputs[:-1] + [gate("performance", "WARN", OTHER)], "rc", approval
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        manifest = release_evidence.build_manifest(
            SHA, inputs[:-1] + [gate("performance", "WARN", url="")], "rc", approval
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        manifest = release_evidence.build_manifest(
            SHA, inputs[:-1] + [gate("performance", "FAIL")], "rc", approval
        )
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        with self.assertRaises(ValueError):
            release_evidence.build_manifest(SHA, inputs, "stable", approval)

    def test_stable_requires_extension_install_version_and_performance(self) -> None:
        names = ("extension", "clean_install", "version_compat", "performance")
        for name in names:
            with self.subTest(gate=name):
                inputs = [gate(key, "PASS") for key in release_evidence.GATES if key != name]
                manifest = release_evidence.build_manifest(SHA, inputs)
                self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
                self.assertEqual(manifest["reasons"], [f"{name}: missing gate"])
        inputs = [gate(key, "WARN" if key == "performance" else "PASS") for key in release_evidence.GATES]
        manifest = release_evidence.build_manifest(SHA, inputs)
        self.assertEqual(manifest["release_decision"], "DO_NOT_RELEASE")
        self.assertIn("performance: WARN", manifest["reasons"][0])

    def test_require_release_returns_failure_and_preserves_manifest(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output):
            status = release_evidence.main(
                ["--target-sha", SHA, "--profile", "rc", "--require-release"]
            )
        self.assertEqual(status, 1)
        self.assertEqual(json.loads(output.getvalue())["release_decision"], "DO_NOT_RELEASE")

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

    def test_beta_profile_requires_extension_but_retains_optional_platforms(self) -> None:
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
