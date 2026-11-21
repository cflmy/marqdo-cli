#!/usr/bin/env python3
"""Aggregate per-gate CI evidence into a SHA-bound release manifest.

Schema: marqdo.release-gates.v1. A missing, skipped, timed-out, failed or
SHA-mismatched gate is never PASS. The tool only records supplied facts; it
does not query CI and cannot turn an UNVERIFIED gate into a release.

Gate spec: `name=STATE:tested_sha:run_url` (url may contain colons).
Required gates: linux, windows, macos, strict_vm; performance must be PASS or
WARN. Any other gate name is rejected so a typo cannot silently disappear.
"""

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

SCHEMA = "marqdo.release-gates.v1"
REQUIRED = ("linux", "windows", "macos", "strict_vm")
OPTIONAL = ("performance",)
STATES = {
    "linux": {"PASS", "FAIL", "UNVERIFIED"},
    "windows": {"PASS", "FAIL", "UNVERIFIED"},
    "macos": {"PASS", "FAIL", "UNVERIFIED"},
    "strict_vm": {"PASS", "FAIL", "UNVERIFIED"},
    "performance": {"PASS", "WARN", "FAIL", "UNVERIFIED"},
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def utc_now() -> str:
    return (
        datetime.datetime.now(datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def parse_gate(text: str) -> tuple[str, dict]:
    if "=" not in text:
        raise ValueError(f"gate must be name=STATE:sha:url, got {text!r}")
    name, rest = text.split("=", 1)
    parts = rest.split(":", 2)
    if len(parts) < 3:
        raise ValueError(f"gate must be name=STATE:sha:url, got {text!r}")
    state, tested_sha, run_url = parts
    if name not in STATES:
        raise ValueError(f"unknown gate {name!r}; expected {list(STATES)}")
    if state not in STATES[name]:
        raise ValueError(f"invalid state {state!r} for gate {name!r}")
    if tested_sha and not SHA_RE.fullmatch(tested_sha):
        raise ValueError(f"gate {name!r} tested_sha is not a full 40-hex SHA: {tested_sha!r}")
    return name, {"state": state, "tested_sha": tested_sha, "run_url": run_url}


def build_manifest(target_sha: str, gates: list[str]) -> dict:
    if not SHA_RE.fullmatch(target_sha):
        raise ValueError(f"target SHA must be 40 lowercase hex characters: {target_sha!r}")
    parsed = {}
    for spec in gates:
        name, gate = parse_gate(spec)
        if name in parsed:
            raise ValueError(f"duplicate gate {name!r}")
        parsed[name] = gate

    reasons = []
    for name in REQUIRED + OPTIONAL:
        gate = parsed.get(name)
        if gate is None:
            parsed[name] = {"state": "UNVERIFIED", "tested_sha": "", "run_url": ""}
            reasons.append(f"{name}: missing gate")
            continue
        if gate["tested_sha"] != target_sha:
            reasons.append(
                f"{name}: tested_sha {gate['tested_sha'] or '(missing)'} != target_sha {target_sha}"
            )
        elif gate["state"] == "FAIL":
            reasons.append(f"{name}: FAIL")
        elif gate["state"] == "UNVERIFIED":
            reasons.append(f"{name}: UNVERIFIED")
        elif name in REQUIRED and gate["state"] != "PASS":
            reasons.append(f"{name}: {gate['state']} is not PASS")
        elif name == "performance" and gate["state"] not in {"PASS", "WARN"}:
            reasons.append(f"performance: {gate['state']} is not PASS/WARN")

    decision = "RELEASE_CANDIDATE" if not reasons else "DO_NOT_RELEASE"
    recorded = utc_now()
    return {
        "schema": SCHEMA,
        "target_sha": target_sha,
        "recorded_utc": recorded,
        "gates": {
            name: {
                **gate,
                "recorded_utc": recorded,
            }
            for name, gate in parsed.items()
        },
        "release_decision": decision,
        "reasons": reasons,
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--gate", action="append", default=[])
    parser.add_argument("--out", default="")
    args = parser.parse_args(argv)
    try:
        manifest = build_manifest(args.target_sha, args.gate)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    text = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text)
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
