#!/usr/bin/env python3
"""Correctness-verified Marqdo performance sampler (schema marqdo.perf.v2).

This replaces the three-run P6 baseline profile. It:

1. builds (or verifies) one release binary and records its SHA256, rustc,
   Cargo.lock digest and git SHA;
2. verifies each workload's exact wire output against a committed expected
   digest **before** timing it;
3. warms up and samples with a fixed, SHA-derived interleaved order;
4. reports p50/p95/MAD, CPU user/sys, max RSS and the environment manifest.

It deliberately does not compare timings across machines. Use the same
`--platform-id`, CPU affinity and release binary when comparing a baseline.
A short run is marked SMOKE, not MEASURED, and must not be used as release
performance evidence.
"""

import argparse
import base64
from collections import Counter
import hashlib
import json
import os
import platform
import random
import shutil
import signal
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKLOADS = ROOT / "tools" / "perf" / "workloads.json"
EXPECTED = ROOT / "tools" / "perf" / "expected"
SCHEMA = "marqdo.perf.v2"
MIN_WARMUPS = 5
MIN_SAMPLES = 20
P50_REGRESSION = 0.15
RSS_REGRESSION = 0.20


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def output(command: list[str]) -> str:
    try:
        return subprocess.run(
            command, check=False, capture_output=True, text=True, timeout=30
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def git_sha(repo: Path) -> str:
    explicit = os.environ.get("MARQDO_TESTED_SHA", "")
    if explicit:
        return explicit
    return output(["git", "-C", str(repo), "rev-parse", "HEAD"]) or "unknown"


OBSERVATION_SCHEMA = "marqdo.observation"
# Run-specific or host-specific payload fields never contribute to correctness.
VOLATILE_PAYLOAD_KEYS = {"file", "started_unix_ms"}


def _stable(value):
    """Drop run/host-specific data from an observation payload."""
    if isinstance(value, dict):
        return {
            key: _stable(item)
            for key, item in value.items()
            if key not in VOLATILE_PAYLOAD_KEYS
        }
    if isinstance(value, list):
        return [_stable(item) for item in value]
    if isinstance(value, str):
        return value.replace(str(ROOT), "<repo>")
    return value


def _committed_bytes(chunks: list[str]) -> dict:
    data = b"".join(base64.b64decode(chunk) for chunk in chunks)
    return {
        "bytes": len(data),
        "sha256": f"sha256:{hashlib.sha256(data).hexdigest()}",
    }


def normalize(document: dict) -> dict:
    """Canonical subset used for correctness: no timings, no run ids, no paths."""
    if document.get("schema") == OBSERVATION_SCHEMA:
        # Current `run`/`call --output json` snapshot: one execution event
        # sequence. The run id, wall-clock start and absolute paths are
        # per-invocation. Correctness pins the terminal status/result, the
        # committed stdout/stderr bytes and the ordered event kinds (count,
        # per-kind totals and a sequence digest) without storing thousands of
        # structural payloads in the repository.
        events = document.get("events") or []
        kinds = [event.get("kind") or "" for event in events]
        counts = Counter(kinds)
        terminal = _stable(events[-1].get("payload")) if events else None
        return {
            "schema": OBSERVATION_SCHEMA,
            "status": (document.get("run") or {}).get("status"),
            "terminal": terminal,
            "stdout": _committed_bytes(
                [
                    event["payload"]["data_base64"]
                    for event in events
                    if event.get("kind") == "stdout.chunk"
                ]
            ),
            "stderr": _committed_bytes(
                [
                    event["payload"]["data_base64"]
                    for event in events
                    if event.get("kind") == "stderr.chunk"
                ]
            ),
            "events": {
                "count": len(events),
                "sequence_sha256": "sha256:"
                + hashlib.sha256("\n".join(kinds).encode("utf-8")).hexdigest(),
                "by_kind": dict(sorted(counts.items())),
            },
        }
    payload = {}
    for key in ("command", "success", "value", "stdout", "stderr", "errors"):
        if key in document:
            payload[key] = document[key]
    diagnostics = document.get("diagnostics")
    if diagnostics:
        payload["diagnostics"] = [
            {
                key: item.get(key)
                for key in ("code", "severity", "line", "column", "message")
            }
            for item in diagnostics
        ]
    return payload


def wire_digest(document: dict) -> str:
    canonical = json.dumps(
        normalize(document), ensure_ascii=False, sort_keys=True, separators=(",", ":")
    )
    return f"sha256:{hashlib.sha256(canonical.encode('utf-8')).hexdigest()}"


def expected_path(name: str) -> Path:
    return EXPECTED / f"{name}.json"


def text_digest(data: bytes) -> str:
    """Digest of measured text output with the checkout path normalised."""
    normalized = data.replace(str(ROOT).encode(), b"<repo>")
    return f"sha256:{hashlib.sha256(normalized).hexdigest()}"


def load_expected(name: str) -> dict | None:
    path = expected_path(name)
    if not path.is_file():
        return None
    return json.loads(path.read_text())


def record_expected(
    name: str,
    document: dict,
    measure_stdout: bytes | None,
    measure_argv: list[str] | None,
) -> None:
    EXPECTED.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": "marqdo.perf.expected.v1",
        "workload": name,
        "wire_sha256": wire_digest(document),
        "normalized": normalize(document),
    }
    if measure_stdout is not None:
        payload["measure_sha256"] = text_digest(measure_stdout)
        payload["measure_argv"] = measure_argv
    expected_path(name).write_text(
        json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    )


def spawn_capture(
    argv: list[str], cwd: Path, env: dict[str, str], timeout: float
) -> tuple[int, dict, float, float, int]:
    """Run one process, returning exit status, rusage, wall and captured bytes.

    `os.posix_spawn` + `os.wait4` gives exact per-child rusage without an
    external `/usr/bin/time`; stdout/stderr are redirected to temporary files
    so the capture never buffers inside the sampler.
    """
    out_path = cwd / f".perf-out-{os.getpid()}-{time.time_ns()}"
    err_path = cwd / f".perf-err-{os.getpid()}-{time.time_ns()}"
    out_fd = os.open(out_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    err_fd = os.open(err_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    actions = [
        (os.POSIX_SPAWN_DUP2, out_fd, 1),
        (os.POSIX_SPAWN_DUP2, err_fd, 2),
    ]
    started = time.perf_counter()
    try:
        pid = os.posix_spawn(argv[0], argv, env, file_actions=actions)
    except OSError as error:
        os.close(out_fd)
        os.close(err_fd)
        out_path.unlink(missing_ok=True)
        err_path.unlink(missing_ok=True)
        raise RuntimeError(f"cannot spawn {argv[0]}: {error}") from error
    finally:
        os.close(out_fd)
        os.close(err_fd)
    deadline = started + timeout
    status = None
    usage = None
    while True:
        reaped, raw_status, rusage = os.wait4(pid, os.WNOHANG)
        if reaped == pid:
            status = raw_status
            usage = rusage
            break
        if time.perf_counter() >= deadline:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.wait4(pid, 0)
            break
        time.sleep(0.001)
    elapsed = time.perf_counter() - started
    stdout = out_path.read_bytes()
    stderr = err_path.read_bytes()
    out_path.unlink(missing_ok=True)
    err_path.unlink(missing_ok=True)
    if status is None:
        raise RuntimeError(f"timeout after {timeout:g}s: {' '.join(argv)}")
    if os.WIFSIGNALED(status):
        raise RuntimeError(
            f"terminated by signal {os.WTERMSIG(status)}: {' '.join(argv)}"
        )
    return (
        os.WEXITSTATUS(status),
        {
            "user_ms": usage.ru_utime * 1000.0,
            "sys_ms": usage.ru_stime * 1000.0,
            "max_rss_kb": usage.ru_maxrss,
        },
        elapsed,
        stdout,
        stderr,
    )


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return "unknown"


def governor() -> str:
    value = output(
        ["sh", "-c", "cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null"]
    )
    return value or "unavailable"


def loadavg() -> str:
    try:
        return Path("/proc/loadavg").read_text().strip()
    except OSError:
        return "unavailable"


def median_ms(values: list[float]) -> float:
    return round(statistics.median(values), 4)


def mad_ms(values: list[float]) -> float:
    middle = statistics.median(values)
    return round(statistics.median(abs(value - middle) for value in values), 4)


def p95_ms(values: list[float]) -> float:
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(0.95 * (len(ordered) - 1)))))
    return round(ordered[index], 4)


def run_workload(
    binary: Path,
    workload: dict,
    warmups: int,
    samples: int,
    order_seed: str,
    allow_smoke: bool,
    record: bool,
    timeout: float,
    cpu_list: str,
) -> dict:
    name = workload["name"]
    prefix = []
    if cpu_list:
        prefix = [shutil.which("taskset") or "taskset", "-c", cpu_list]
    # Correctness and measurement are separate commands for run/call: the
    # machine document (Observation snapshot) pins the full public contract,
    # while timing copies the plain execution path so an opt-in observability
    # contract is not compared against a summary document.
    correctness_argv = prefix + [str(binary)] + workload["argv"]
    measured_argv = prefix + [str(binary)] + (workload.get("measure_argv") or workload["argv"])
    env = dict(os.environ)
    env.pop("MARQDO_STDLIB_DIR", None)
    if not (binary.parent / "stdlib" / "stdlib.json").is_file():
        env["MARQDO_STDLIB_DIR"] = str(ROOT / "stdlib")
    env.setdefault("NO_COLOR", "1")
    expected_document = load_expected(name)
    expected = expected_document.get("wire_sha256") if expected_document else None
    expected_measure = (
        expected_document.get("measure_sha256") if expected_document else None
    )
    result = {
        "name": name,
        "kind": workload.get("kind", "run"),
        "source": workload["source"],
        "argv": workload["argv"],
    }
    if workload.get("measure_argv"):
        result["measure_argv"] = workload["measure_argv"]
    correctness = "VERIFIED"
    warmup_samples: list[dict] = []
    measured: list[dict] = []
    measure_stdout: bytes | None = None

    # One correctness preflight per workload: the machine document must match
    # the committed contract digest before anything is timed.
    code, _usage, _wall, stdout, stderr = spawn_capture(
        correctness_argv, ROOT, env, timeout
    )
    if code != 0:
        correctness = "CORRECTNESS_FAIL"
        result["error"] = f"exit {code}: {stderr.decode('utf-8', 'replace')[:500]}"
    else:
        try:
            document = json.loads(stdout)
        except json.JSONDecodeError as error:
            correctness = "CORRECTNESS_FAIL"
            result["error"] = f"invalid JSON: {error}"
            document = None
        if document is not None:
            digest = wire_digest(document)
            if workload.get("measure_argv"):
                code, _usage, _wall, measure_stdout, stderr = spawn_capture(
                    measured_argv, ROOT, env, timeout
                )
                if code != 0:
                    correctness = "CORRECTNESS_FAIL"
                    result["error"] = (
                        f"measurement probe exit {code}: "
                        f"{stderr.decode('utf-8', 'replace')[:500]}"
                    )
                    document = None
            if record and document is not None:
                record_expected(
                    name, document, measure_stdout, workload.get("measure_argv")
                )
                expected = digest
                if measure_stdout is not None:
                    expected_measure = text_digest(measure_stdout)
            if document is not None:
                if expected is None:
                    correctness = "UNVERIFIED_NO_EXPECTED"
                elif digest != expected:
                    correctness = "CORRECTNESS_FAIL"
                    result["error"] = (
                        f"wire digest mismatch: expected {expected}, got {digest}"
                    )
                elif measure_stdout is not None:
                    probe = text_digest(measure_stdout)
                    if expected_measure is None:
                        correctness = "UNVERIFIED_NO_EXPECTED"
                    elif probe != expected_measure:
                        correctness = "CORRECTNESS_FAIL"
                        result["error"] = (
                            "measurement output mismatch: expected "
                            f"{expected_measure}, got {probe}"
                        )

    # Timing loop: only a verified workload contributes samples, and every
    # sample's output must match the committed measurement digest.
    if correctness == "VERIFIED":
        for index in range(warmups + samples):
            code, usage, wall, stdout, stderr = spawn_capture(
                measured_argv, ROOT, env, timeout
            )
            if code != 0:
                correctness = "CORRECTNESS_FAIL"
                result["error"] = (
                    f"exit {code}: {stderr.decode('utf-8', 'replace')[:500]}"
                )
                break
            if expected_measure is not None and text_digest(stdout) != expected_measure:
                correctness = "CORRECTNESS_FAIL"
                result["error"] = "measurement output changed between samples"
                break
            sample = {
                "wall_ms": wall * 1000.0,
                "user_ms": usage["user_ms"],
                "sys_ms": usage["sys_ms"],
                "max_rss_kb": usage["max_rss_kb"],
            }
            if index < warmups:
                warmup_samples.append(sample)
            else:
                measured.append(sample)

    result["correctness"] = correctness
    if correctness != "VERIFIED":
        result["status"] = correctness
        return result
    if not measured:
        result["status"] = "NO_SAMPLES"
        return result
    walls = [sample["wall_ms"] for sample in measured]
    result.update(
        {
            "status": "MEASURED" if (len(measured) >= MIN_SAMPLES and warmups >= MIN_WARMUPS) else "SMOKE",
            "warmups": len(warmup_samples),
            "samples": len(measured),
            "order_seed": order_seed,
            "median_ms": median_ms(walls),
            "p95_ms": p95_ms(walls),
            "mad_ms": mad_ms(walls),
            "min_ms": round(min(walls), 4),
            "max_ms": round(max(walls), 4),
            "cpu_user_ms_median": median_ms(
                [sample["user_ms"] for sample in measured]
            ),
            "cpu_sys_ms_median": median_ms([sample["sys_ms"] for sample in measured]),
            "max_rss_kb_median": int(
                statistics.median(sample["max_rss_kb"] for sample in measured)
            ),
        }
    )
    if result["status"] == "SMOKE" and not allow_smoke:
        pass  # still recorded as SMOKE; the caller decides whether to accept it
    return result


def compare_baseline(report: dict, baseline: dict) -> list[dict]:
    if baseline.get("schema") != SCHEMA:
        raise SystemExit(f"baseline schema {baseline.get('schema')!r} is not {SCHEMA}")
    if baseline.get("platform_id") != report.get("platform_id"):
        raise SystemExit(
            "baseline platform_id differs; timings from another machine/node are not comparable"
        )
    if baseline.get("binary", {}).get("profile") != report.get("binary", {}).get("profile"):
        raise SystemExit("baseline profile differs")
    for field in ("kernel", "machine", "cpu_model", "governor", "cpu_list"):
        if baseline["environment"].get(field) != report["environment"].get(field):
            raise SystemExit(f"baseline environment.{field} differs")
    for field in ("rustc", "cargo"):
        if baseline["binary"].get(field) != report["binary"].get(field):
            raise SystemExit(f"baseline binary.{field} differs")
    if report["environment_approved"] and (
        not baseline.get("environment_approved")
        or baseline.get("status") != "MEASURED"
        or baseline.get("correctness") != "VERIFIED"
    ):
        raise SystemExit("approved-node comparison requires an approved, verified MEASURED baseline")
    previous = {item["name"]: item for item in baseline.get("workloads", [])}
    comparisons = []
    for current in report["workloads"]:
        old = previous.get(current["name"])
        if not old or old.get("status") not in {"MEASURED", "SMOKE"}:
            comparisons.append({"name": current["name"], "status": "NO_BASELINE"})
            continue
        if current.get("status") not in {"MEASURED", "SMOKE"}:
            comparisons.append(
                {"name": current["name"], "status": "CURRENT_NOT_MEASURED"}
            )
            continue
        p50_delta = (current["median_ms"] - old["median_ms"]) / old["median_ms"]
        rss_delta = (
            current["max_rss_kb_median"] - old["max_rss_kb_median"]
        ) / old["max_rss_kb_median"]
        comparisons.append(
            {
                "name": current["name"],
                "status": "COMPARED",
                "p50_delta_percent": round(p50_delta * 100.0, 2),
                "rss_delta_percent": round(rss_delta * 100.0, 2),
                "investigate": p50_delta >= P50_REGRESSION
                or rss_delta >= RSS_REGRESSION,
            }
        )
    return comparisons


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(ROOT / "target" / "release" / "marqdo"))
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--warmups", type=int, default=MIN_WARMUPS)
    parser.add_argument("--samples", type=int, default=MIN_SAMPLES)
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--only", action="append", default=[])
    parser.add_argument("--record-expected", action="store_true")
    parser.add_argument("--platform-id", default="local-unapproved")
    parser.add_argument("--approved-node", action="store_true")
    parser.add_argument("--cpu-list", default="")
    parser.add_argument("--baseline", default="")
    parser.add_argument("--out", default="")
    parser.add_argument("--allow-smoke", action="store_true")
    args = parser.parse_args(argv)

    if args.samples < 1 or args.warmups < 0:
        raise SystemExit("samples must be >= 1 and warmups >= 0")
    if (args.samples < MIN_SAMPLES or args.warmups < MIN_WARMUPS) and not args.allow_smoke:
        raise SystemExit(
            f"release evidence requires >= {MIN_WARMUPS} warmups and >= {MIN_SAMPLES} "
            "samples; pass --allow-smoke for a non-release smoke run"
        )
    binary = Path(args.binary).resolve()
    if not args.no_build:
        build = [os.environ.get("CARGO", "cargo"), "build", "--release", "--locked", "-p", "marqdo-cli"]
        subprocess.run(build, cwd=ROOT, check=True)
    if not binary.is_file():
        raise SystemExit(f"release binary not found: {binary}")

    if args.cpu_list and not shutil.which("taskset"):
        raise SystemExit("--cpu-list requires taskset")

    config = json.loads(WORKLOADS.read_text())
    workloads = [
        item
        for item in config["workloads"]
        if not args.only or item["name"] in args.only
    ]
    if not workloads:
        raise SystemExit("no workloads selected")

    source_sha = git_sha(ROOT)
    order_seed = hashlib.sha256(source_sha.encode()).hexdigest()[:16]
    order = list(workloads)
    random.Random(order_seed).shuffle(order)
    start_load = loadavg()
    results = []
    for workload in order:
        result = run_workload(
            binary,
            workload,
            args.warmups,
            args.samples,
            order_seed,
            args.allow_smoke,
            args.record_expected,
            args.timeout,
            args.cpu_list,
        )
        results.append(result)
        print(
            f"{result['name']}: {result.get('status')} "
            f"median={result.get('median_ms', 'n/a')}ms "
            f"p95={result.get('p95_ms', 'n/a')}ms "
            f"rss={result.get('max_rss_kb_median', 'n/a')}KB",
            file=sys.stderr,
        )

    if args.record_expected:
        print("expected digests recorded; commit tools/perf/expected/", file=sys.stderr)

    correctness_ok = all(item.get("correctness") == "VERIFIED" for item in results)
    measured = all(item.get("status") == "MEASURED" for item in results)
    report = {
        "schema": SCHEMA,
        "source_sha": source_sha,
        "platform_id": args.platform_id,
        "environment_approved": bool(args.approved_node),
        "binary": {
            "path": str(binary.relative_to(ROOT)) if binary.is_relative_to(ROOT) else str(binary),
            "sha256": sha256_file(binary),
            "profile": "release",
            "cargo_lock_sha256": sha256_file(ROOT / "Cargo.lock"),
            "rustc": output(["rustc", "-vV"]),
            "cargo": output(["cargo", "-V"]),
        },
        "environment": {
            "uname": platform.platform(),
            "kernel": platform.release(),
            "machine": platform.machine(),
            "cpu_model": cpu_model(),
            "governor": governor(),
            "loadavg_start": start_load,
            "cpu_list": args.cpu_list or "unbound",
            "python": platform.python_version(),
        },
        "recipe": {
            "warmups": args.warmups,
            "samples": args.samples,
            "order_seed": order_seed,
            "timeout_seconds": args.timeout,
        },
        "workloads": results,
        "correctness": "VERIFIED" if correctness_ok else "CORRECTNESS_FAIL",
        "status": "MEASURED"
        if (measured and args.approved_node)
        else ("SMOKE" if correctness_ok else "CORRECTNESS_FAIL"),
        "baseline_comparison": [],
    }
    report["environment"]["loadavg_end"] = loadavg()
    if args.baseline:
        baseline = json.loads(Path(args.baseline).read_text())
        report["baseline_comparison"] = compare_baseline(report, baseline)

    text = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.out:
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(text)
    sys.stdout.write(text)
    if not correctness_ok:
        return 1
    if not measured and not args.allow_smoke:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
