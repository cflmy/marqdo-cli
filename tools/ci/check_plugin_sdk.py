#!/usr/bin/env python3
"""Real third-party plugin SDK consumption gate (issue #13 §6.3, S04-S05).

Builds the fixture in ``tests/sdk-plugin`` with only the public Rust SDK, creates
the host-specific ``plugin.yaml``, then lets a real built CLI load and invoke it
with explicit trust and an explicit plugin grant. A tampered digest must fail
closed. No mock host, no direct Rust call into plugin internals.

Usage:
    python3 tools/ci/check_plugin_sdk.py [--offline]
"""

from __future__ import annotations

import argparse
import hashlib
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE = REPO_ROOT / "tests" / "sdk-plugin"
CALL_DOC = FIXTURE / "call.mq.md"


class GateError(Exception):
    pass


def cargo() -> str:
    path = shutil.which("cargo")
    if path is None:
        raise GateError("cargo is not on PATH")
    return path


def run(command: list[str], *, check: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(
        command, cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )
    if check and result.returncode != 0:
        raise GateError(
            f"`{' '.join(command)}` failed with {result.returncode}:\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return result


def host_triple() -> str:
    result = run(["rustc", "-vV"])
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line[len("host: ") :].strip()
    raise GateError("rustc -vV did not report a host triple")


def artifact_name() -> str:
    system = platform.system()
    if system == "Darwin":
        return "libmarqdo_sdk_plugin_fixture.dylib"
    if system == "Windows":
        return "marqdo_sdk_plugin_fixture.dll"
    return "libmarqdo_sdk_plugin_fixture.so"


def write_manifest(
    directory: Path,
    digest: str,
    triple: str,
    artifact: str,
    *,
    tamper: bool = False,
) -> None:
    effective = "0" * 64 if tamper else digest
    manifest = f"""plugin:
  name: sdk.demo
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: sdk.add
      signature: "($a: Int, $b: Int) -> Int"
    - name: sdk.echo
      signature: "($text: Text) -> Text"
  artifacts:
    - target: {triple}
      file: {artifact}
      sha256: "{effective}"
"""
    (directory / "plugin.yaml").write_text(manifest, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument(
        "--sdk-revision",
        default=None,
        help="informational: exact CLI/SDK source revision under test",
    )
    arguments = parser.parse_args()
    offline = ["--offline"] if arguments.offline else []
    try:
        run([cargo(), "build", "--locked", "-p", "marqdo-cli", *offline])
        run([cargo(), "build", "--locked", "--manifest-path", str(FIXTURE / "Cargo.toml"), *offline])
        name = artifact_name()
        built = FIXTURE / "target" / "debug" / name
        if not built.is_file():
            raise GateError(f"SDK fixture artifact not found: {built}")
        triple = host_triple()
        binary = REPO_ROOT / "target" / "debug" / "marqdo"
        if not binary.is_file():
            raise GateError(f"CLI binary not found: {binary}")

        with tempfile.TemporaryDirectory(prefix="marqdo-sdk-plugin-") as raw:
            directory = Path(raw)
            artifact = "plugin" + Path(name).suffix
            shutil.copyfile(built, directory / artifact)
            digest = hashlib.sha256((directory / artifact).read_bytes()).hexdigest()
            command = [
                str(binary),
                "call",
                str(CALL_DOC),
                "main",
                "--plugin",
                str(directory),
                "--trust-native-plugin",
                "--allow-plugin",
                "sdk.demo",
            ]

            write_manifest(directory, digest, triple, artifact)
            positive = run(command, check=False)
            if positive.returncode != 0 or positive.stdout.strip() != "42":
                raise GateError(
                    "real CLI could not invoke the SDK plugin: "
                    f"exit={positive.returncode} stdout={positive.stdout!r} "
                    f"stderr={positive.stderr!r}"
                )
            print("PASS sdk-plugin-real-invocation (Int 42)")

            write_manifest(directory, digest, triple, artifact, tamper=True)
            negative = run(command, check=False)
            if negative.returncode == 0:
                raise GateError("tampered plugin artifact digest was accepted")
            combined = negative.stdout + negative.stderr
            if "sha256" not in combined.lower() and "artifact" not in combined.lower():
                raise GateError(
                    "tampered artifact failed without an artifact/sha256 diagnostic: "
                    + combined
                )
            print("PASS sdk-plugin-tampered-digest-rejected")
    except GateError as error:
        print(f"FAIL plugin-sdk: {error}", file=sys.stderr)
        return 1
    print("PASS plugin-sdk")
    return 0


if __name__ == "__main__":
    sys.exit(main())
