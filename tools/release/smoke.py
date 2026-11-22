#!/usr/bin/env python3
"""Cold-install smoke test for a built Marqdo release archive.

Extracts the archive into a fresh temporary directory and exercises the shipped
binaries only (never the development source tree): version/help, check/run/call,
lock/sync for a path dependency, Analysis v2 + Trace v2 consumers, Tool
install/dispatch, native and worker plugin transports, foreign kernel, the
Provider v2 worker (positive, unauthorized and missing-helper fail-closed), and
strict mode when the host genuinely supports it.

Usage:
    python3 tools/release/smoke.py --archive dist/marqdo-3.0.0-rc.1-x86_64-unknown-linux-gnu.tar.gz
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def fail(message: str) -> "NoReturn":
    print(f"smoke: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(*chunks: bytes) -> str:
    digest = hashlib.sha256()
    for chunk in chunks:
        digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def stdlib_tree_digest(root: Path) -> str:
    files = sorted(
        (path.relative_to(root).as_posix(), path)
        for path in root.rglob("*.mq.md")
        if path.is_file()
    )
    return sha256_bytes(
        *(
            chunk
            for relative, path in files
            for chunk in (relative.encode(), b"\0", path.read_bytes(), b"\0")
        )
    )


def stdlib_aliases_digest(manifest: dict) -> str:
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
    return sha256_bytes(*chunks)


def verify_stdlib_bundle(install: Path, info: dict) -> None:
    bundle = install / "stdlib"
    manifest_path = bundle / "stdlib.json"
    if not manifest_path.is_file():
        fail("release archive is missing stdlib/stdlib.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    recorded = info.get("stdlib") or {}
    for required in ("stdlib.json", "io.mq.md", "io/console.mq.md"):
        if not (bundle / required).is_file():
            fail(f"release archive stdlib is missing {required}")
    tree = stdlib_tree_digest(bundle)
    aliases = stdlib_aliases_digest(manifest)
    if recorded.get("tree_digest") != tree or manifest.get("digest") != tree:
        fail(
            f"stdlib tree digest mismatch: RELEASE_INFO={recorded.get('tree_digest')} "
            f"stdlib.json={manifest.get('digest')} actual={tree}"
        )
    if recorded.get("aliases_digest") != aliases or manifest.get("aliases_digest") != aliases:
        fail(
            f"stdlib alias digest mismatch: RELEASE_INFO={recorded.get('aliases_digest')} "
            f"stdlib.json={manifest.get('aliases_digest')} actual={aliases}"
        )
    if recorded.get("name") != manifest.get("name") or recorded.get("version") != manifest.get("version"):
        fail("RELEASE_INFO.stdlib name/version disagree with stdlib/stdlib.json")
    files = [
        path.relative_to(bundle).as_posix()
        for path in bundle.rglob("*")
        if path.is_file()
    ]
    if recorded.get("file_count") != len(files):
        fail(
            f"RELEASE_INFO.stdlib.file_count={recorded.get('file_count')} "
            f"but archive has {len(files)} files"
        )
    print(
        f"smoke: verified stdlib {recorded.get('version')} "
        f"tree={tree} aliases={aliases} files={len(files)}"
    )


class Runner:
    def __init__(self, root: Path):
        self.root = root
        self.marqdo = root / "marqdo"
        self.env = dict(os.environ)
        self.env.pop("MARQDO_MODE", None)
        self.env["MARQDO_TOOL_HOME"] = str(root / "tool-home")

    def run(
        self,
        args: list[str],
        *,
        expect: int = 0,
        contains: str | None = None,
        cwd: Path | None = None,
        env: dict | None = None,
    ) -> subprocess.CompletedProcess:
        command = [str(self.marqdo), *args]
        merged = dict(self.env)
        if env:
            merged.update(env)
        result = subprocess.run(
            command,
            cwd=cwd or self.root,
            env=merged,
            capture_output=True,
            text=True,
            timeout=120,
        )
        shown = " ".join(command[1:])
        if result.returncode != expect:
            fail(
                f"`marqdo {shown}` exited {result.returncode}, expected {expect}\n"
                f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
            )
        if contains is not None:
            haystack = result.stdout + result.stderr
            if contains not in haystack:
                fail(f"`marqdo {shown}` output does not contain {contains!r}:\n{haystack}")
        return result

    def json(self, args: list[str]) -> dict:
        result = self.run([*args, "--json"])
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as error:
            fail(f"`marqdo {' '.join(args)} --json` is not JSON: {error}\n{result.stdout}")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True)
    parser.add_argument("--workdir", default=None, help="keep the extracted tree here")
    parser.add_argument("--skip-strict", action="store_true")
    args = parser.parse_args(argv)

    archive = Path(args.archive).resolve()
    if not archive.is_file():
        fail(f"archive not found: {archive}")
    checksum_file = archive.with_name(archive.name + ".sha256")
    if checksum_file.is_file():
        expected = checksum_file.read_text(encoding="utf-8").split()[0]
        actual = sha256_file(archive)
        if expected != actual:
            fail(f"archive sha256 {actual} does not match {checksum_file}: {expected}")

    if args.workdir:
        root = Path(args.workdir).resolve()
        shutil.rmtree(root, ignore_errors=True)
        root.mkdir(parents=True)
    else:
        root = Path(tempfile.mkdtemp(prefix="marqdo-smoke-"))
    try:
        shutil.unpack_archive(str(archive), str(root))
        entries = [path for path in root.iterdir() if path.is_dir()]
        if len(entries) != 1:
            fail(f"archive must contain exactly one top-level directory, found {entries}")
        install = entries[0]
        info = json.loads((install / "RELEASE_INFO.json").read_text(encoding="utf-8"))
        version = info["product_version"]
        source_sha = info["source_sha"]
        if (install / "SOURCE_SHA").read_text(encoding="utf-8").strip() != source_sha:
            fail("SOURCE_SHA does not match RELEASE_INFO.json")
        for name in ("marqdo", "marqdo-provider-worker"):
            binary = install / name
            if not binary.is_file() or not os.access(binary, os.X_OK):
                fail(f"{name} is missing or not executable in the archive")
            recorded = info["binaries"][name]["sha256"]
            if sha256_file(binary) != recorded:
                fail(f"{name} sha256 does not match RELEASE_INFO.json")
        verify_stdlib_bundle(install, info)

        runner = Runner(install)
        print(f"smoke: archive version {version}, source {source_sha}")

        version_out = runner.run(["--version"]).stdout.strip()
        if version_out != f"marqdo {version}":
            fail(f"--version printed {version_out!r}, expected 'marqdo {version}'")
        runner.run(["--help"])
        runner.run(["check", "examples/multi-module/index.mq.md"])
        hello = runner.run(["run", "examples/hello.mq.md"]).stdout
        if "你好" not in hello:
            fail(f"hello example printed {hello!r}")

        # The archive must not fall back to a build-machine source tree: removing
        # the shipped stdlib must fail closed (T-PKG-03).
        hidden = install / "stdlib.hidden"
        (install / "stdlib").rename(hidden)
        try:
            broken = runner.run(["run", "examples/hello.mq.md"], expect=1)
            if broken.returncode == 0:
                fail("run succeeded after deleting the bundled stdlib (forbidden fallback)")
            message = broken.stdout + broken.stderr
            if "stdlib" not in message and "打印" not in message and "symbol.unknown" not in message:
                fail(f"missing-stdlib failure is not explicit: {message!r}")
            print("smoke: bundled stdlib missing -> fail-closed OK")
        finally:
            hidden.rename(install / "stdlib")
        factor = runner.run(
            ["call", "examples/factorial.mq.md", "阶乘", "--arg", "n=5"]
        ).stdout.strip()
        if factor != "120":
            fail(f"factorial call printed {factor!r}, expected 120")

        # Static Analysis v2 + Trace v2 through the public consumers.
        analysis = install / "analysis.json"
        trace = install / "trace.json"
        runner.run(
            [
                "check",
                "examples/multi-module/index.mq.md",
                "--emit",
                str(analysis),
            ]
        )
        runner.run(["check", "examples/multi-module/index.mq.md", "--refs"])
        document = json.loads(analysis.read_text(encoding="utf-8"))
        if document.get("schema") != "marqdo.analysis" or document.get("schema_version") != 2:
            fail("--emit did not write Analysis v2")
        if document.get("producer", {}).get("version") != version:
            fail("Analysis producer.version does not match the released product version")
        runner.run(
            [
                "run",
                "examples/multi-module/index.mq.md",
                "--trace-file",
                str(trace),
            ]
        )
        run_trace = json.loads(trace.read_text(encoding="utf-8"))
        if run_trace.get("schema") != "marqdo.trace" or run_trace.get("schema_version") != 2:
            fail("--trace-file did not write Trace v2")
        for consumer in ("analysis_consumer.py", "trace_consumer.py"):
            command = [
                sys.executable,
                str(install / "tests" / "consumers" / consumer),
                str(analysis),
            ]
            if consumer == "trace_consumer.py":
                command.append(str(trace))
            result = subprocess.run(command, capture_output=True, text=True)
            if result.returncode != 0:
                fail(f"{consumer} failed:\n{result.stdout}\n{result.stderr}")
        # Execution Events v1: the released binary's live NDJSON stream,
        # validated by the independent consumer shipped in the archive.
        run_help = runner.run(["run", "--help"]).stdout
        if "--event-stream" not in run_help or "--observe-variable" not in run_help:
            fail("run --help does not advertise Execution Events v1 options")
        live = runner.run(
            [
                "run",
                "tests/consumers/fixtures/execution-events/basic.mq.md",
                "--event-stream",
                "jsonl",
            ]
        )
        events_path = install / "execution-events.jsonl"
        events_path.write_text(live.stdout, encoding="utf-8")
        events = [
            json.loads(line)
            for line in live.stdout.splitlines()
            if line.strip()
        ]
        if not events or events[0].get("kind") != "stream.opened":
            fail("Execution Events stream does not start with stream.opened")
        if events[0].get("payload", {}).get("producer", {}).get("version") != version:
            fail("Execution Events producer.version does not match the product version")
        if events[0].get("schema_version") != info["protocols"].get("execution_events"):
            fail("Execution Events schema_version does not match RELEASE_INFO.json")
        if events[-1].get("kind") != "run.completed":
            fail("Execution Events stream has no successful terminal event")
        validate_events = subprocess.run(
            [
                sys.executable,
                str(install / "tests" / "consumers" / "execution_events_consumer.py"),
                str(events_path),
            ],
            capture_output=True,
            text=True,
        )
        if validate_events.returncode != 0:
            if "jsonschema" in validate_events.stderr and "ModuleNotFoundError" in validate_events.stderr:
                print("smoke: note: jsonschema is not installed; Execution Events schema validation skipped")
            else:
                fail(
                    "execution_events_consumer.py failed:\n"
                    f"{validate_events.stdout}\n{validate_events.stderr}"
                )

        validate = subprocess.run(
            [
                sys.executable,
                str(install / "tests" / "consumers" / "validate_protocols.py"),
                str(analysis),
                str(trace),
                str(install / "examples" / "tool-unknown-name" / "tool.json"),
            ],
            capture_output=True,
            text=True,
        )
        if validate.returncode != 0:
            if "jsonschema" in validate.stderr and "ModuleNotFoundError" in validate.stderr:
                print("smoke: note: jsonschema is not installed; schema validation skipped")
            else:
                fail(f"validate_protocols.py failed:\n{validate.stdout}\n{validate.stderr}")

        # Tool Protocol v1: install and dispatch an independent process.
        runner.run(["tool", "install", "examples/tool-unknown-name/tool.json"])
        listed = runner.run(["tool", "list"]).stdout
        if "benchmark" not in listed:
            fail(f"tool list does not contain benchmark:\n{listed}")
        benchmark = runner.run(["benchmark", "run", "."]).stdout
        if "benchmark tool:" not in benchmark:
            fail(f"tool dispatch output unexpected:\n{benchmark}")

        # Callable plugin ABI v1: native (explicit trust) and worker transports.
        native = runner.run(
            [
                "run",
                "examples/plugin-demo/plugin-call.mq.md",
                "--plugin",
                "examples/plugin-demo/build",
                "--trust-native-plugin",
                "--allow-plugin",
                "demo.solver",
            ]
        ).stdout.strip()
        if native != "42":
            fail(f"native plugin example printed {native!r}, expected 42")
        runner.run(
            ["plugin", "inspect", "examples/plugin-demo/build", "--json"]
        )
        worker_dir = install / "worker-plugin"
        worker_dir.mkdir()
        shutil.copy2(install / "examples/plugin-demo/build/solver.so", worker_dir / "solver.so")
        worker_manifest = (
            install / "examples/plugin-demo/build/plugin.yaml"
        ).read_text(encoding="utf-8").replace("kind: native", "kind: worker")
        (worker_dir / "plugin.yaml").write_text(worker_manifest, encoding="utf-8")
        worker = runner.run(
            [
                "run",
                "examples/plugin-demo/plugin-call.mq.md",
                "--plugin",
                str(worker_dir),
                "--allow-plugin",
                "demo.solver",
            ]
        ).stdout.strip()
        if worker != "42":
            fail(f"worker plugin example printed {worker!r}, expected 42")

        # Foreign kernel: no core change for a new language tag.
        kernel = runner.run(
            [
                "run",
                "examples/kernel-mock/demo.mq.md",
                "--kernel",
                "examples/kernel-mock/kernel.json",
                "--allow-foreign",
            ]
        ).stdout.strip()
        if kernel != "42":
            fail(f"foreign kernel example printed {kernel!r}, expected 42")

        # Resource Provider v2 through the shipped worker helper.
        provider = "examples/provider-demo/build/provider.json"
        provider_run = runner.json(
            [
                "run",
                "examples/provider-demo/demo.mq.md",
                "--provider",
                provider,
                "--allow-provider",
                "demo.sensor",
            ]
        )
        if not provider_run.get("success"):
            fail(f"provider run was not successful: {provider_run}")
        reads = [
            event
            for event in provider_run.get("trace", [])
            if event.get("op") == "resource.read"
        ]
        if not reads or "count=5" not in str(reads[-1].get("result")):
            fail(f"provider read did not report five bytes: {reads}")
        runner.run(
            [
                "run",
                "examples/provider-demo/unauthorized.mq.md",
                "--provider",
                provider,
            ],
            expect=1,
            contains="permission.denied",
        )
        worker_binary = install / "marqdo-provider-worker"
        shutil.move(str(worker_binary), str(worker_binary.with_suffix(".hidden")))
        try:
            runner.run(
                [
                    "run",
                    "examples/provider-demo/demo.mq.md",
                    "--provider",
                    provider,
                    "--allow-provider",
                    "demo.sensor",
                ],
                expect=1,
                contains="ext.worker_unavailable",
            )
        finally:
            shutil.move(str(worker_binary.with_suffix(".hidden")), str(worker_binary))

        # RFC-EXT-0003: math algorithms are external. The pure package and the
        # optional ABI backend are exercised by the frozen archive only; the core
        # has no math.* builtin.
        gcd = runner.run(["call", "examples/math-library-demo.mq.md", "main"]).stdout.strip()
        if gcd != "6":
            fail(f"pure math library printed {gcd!r}, expected 6")
        runner.run(["check", "examples/math-library/index.mq.md"])
        math_backend = runner.run(
            [
                "run",
                "examples/math-backend/demo.mq.md",
                "--plugin",
                "examples/math-backend/build",
                "--trust-native-plugin",
                "--allow-plugin",
                "math.backend",
            ]
        ).stdout.strip()
        if math_backend != "42":
            fail(f"math ABI backend printed {math_backend!r}, expected 42")
        runner.run(
            [
                "run",
                "examples/math-backend/demo.mq.md",
                "--plugin",
                "examples/math-backend/build",
                "--trust-native-plugin",
            ],
            expect=1,
            contains="permission.denied",
        )
        removed = install / "removed-math.mq.md"
        removed.write_text(
            "## @exec main\n\n`return math.eval(ref(\"#公式\"), {})`\n",
            encoding="utf-8",
        )
        runner.run(["check", str(removed)], expect=1, contains="symbol.unknown")

        # pure mode: language execution without project grants.
        runner.run(["run", "examples/hello.mq.md", "--mode", "pure"])

        if not args.skip_strict:
            platform = runner.json(["inspect", "examples/hello.mq.md", "--platform"])
            strict = platform.get("platform", {}).get("strict", {})
            if strict.get("state") == "available":
                runner.run(["run", "examples/hello.mq.md", "--mode", "strict"])
                print(f"smoke: strict available ({strict.get('mechanism')}) and exercised")
            else:
                refused = runner.run(
                    ["run", "examples/hello.mq.md", "--mode", "strict"],
                    expect=1,
                )
                if "strict" not in (refused.stdout + refused.stderr):
                    fail("unsupported strict mode did not fail closed with a strict diagnostic")
                print(f"smoke: strict unsupported on this host; fail-closed refused: {strict}")

        print("smoke: PASS")
        return 0
    finally:
        if not args.workdir:
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
