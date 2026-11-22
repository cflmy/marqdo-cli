#!/usr/bin/env python3
"""Aggregate per-gate CI evidence into a SHA-bound release manifest.

Schema: `marqdo.release-gates.v1`. A missing, skipped, timed-out, failed or
SHA-mismatched gate is never PASS. The tool only records supplied facts; it
does not query CI and cannot turn an UNVERIFIED gate into a release.

The `rc` profile requires every gate on the target SHA. For a Linux-only RC,
Windows and macOS must be explicitly UNSUPPORTED; the stable profile requires
both to PASS. Strict VM, extension, clean install, version compatibility and
performance require PASS. RC can record a shared-node performance WARN only
with an explicit maintainer approval reference. The historical beta profile
retains its own scoped exceptions.

Gate spec: `name=STATE:tested_sha:run_url` (url may contain colons).

Usage:

    python3 tools/ci/release_evidence.py --target-sha "$SHA" --profile beta \
        --gate linux=PASS:$SHA:https://ci.cflmy.cn/... \
        --gate strict_vm=PASS:$SHA:docs/evidence/... \
        --gate extension=PASS:$SHA:docs/evidence/extension/$SHA/manifest.json \
        --gate clean_install=PASS:$SHA:docs/evidence/... \
        --gate version_compat=PASS:$SHA:cargo-test \
        --gate windows=UNSUPPORTED:$SHA:supported-platforms-exclude \
        --gate macos=UNSUPPORTED:$SHA:supported-platforms-exclude \
        --gate performance=WARN:$SHA:docs/evidence/... \
        --out docs/evidence/ci/$SHA/manifest.json
"""

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

SCHEMA = "marqdo.release-gates.v1"
GATES = (
    "linux",
    "windows",
    "macos",
    "strict_vm",
    "extension",
    "clean_install",
    "version_compat",
    "performance",
)
RC_PLATFORM_STATES = {"windows": {"UNSUPPORTED"}, "macos": {"UNSUPPORTED"}}
BETA_OPTIONAL = ("windows", "macos", "performance")
PROFILES = {
    "stable": ("RELEASE_CANDIDATE", {}, ()),
    "rc": ("RC_RELEASE_CANDIDATE", RC_PLATFORM_STATES, ()),
    "beta": (
        "BETA_RELEASE_CANDIDATE",
        {
            "windows": {"PASS", "UNSUPPORTED"},
            "macos": {"PASS", "UNSUPPORTED"},
            "strict_vm": {"PASS", "UNSUPPORTED"},
            "performance": {"PASS", "WARN"},
        },
        BETA_OPTIONAL,
    ),
}
STATES = {
    "linux": {"PASS", "FAIL", "UNVERIFIED"},
    "windows": {"PASS", "FAIL", "UNVERIFIED", "UNSUPPORTED"},
    "macos": {"PASS", "FAIL", "UNVERIFIED", "UNSUPPORTED"},
    "strict_vm": {"PASS", "FAIL", "UNVERIFIED", "UNSUPPORTED"},
    "extension": {"PASS", "FAIL", "UNVERIFIED"},
    "clean_install": {"PASS", "FAIL", "UNVERIFIED"},
    "version_compat": {"PASS", "FAIL", "UNVERIFIED"},
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
        raise ValueError(f"unknown gate {name!r}; expected {list(GATES)}")
    if state not in STATES[name]:
        raise ValueError(f"invalid state {state!r} for gate {name!r}")
    if tested_sha and not SHA_RE.fullmatch(tested_sha):
        raise ValueError(f"gate {name!r} tested_sha is not a full 40-hex SHA: {tested_sha!r}")
    return name, {"state": state, "tested_sha": tested_sha, "run_url": run_url}


def build_manifest(
    target_sha: str,
    gates: list[str],
    profile: str = "stable",
    rc_shared_performance_approval: str = "",
) -> dict:
    if not SHA_RE.fullmatch(target_sha):
        raise ValueError(f"target SHA must be 40 lowercase hex characters: {target_sha!r}")
    try:
        release_decision, accepted_states, optional = PROFILES[profile]
    except KeyError:
        raise ValueError(f"unknown profile {profile!r}; expected {sorted(PROFILES)}") from None
    approval = rc_shared_performance_approval.strip()
    if approval and profile != "rc":
        raise ValueError("shared-node approval is supported only by the rc profile")
    if approval:
        accepted_states = {**accepted_states, "performance": {"PASS", "WARN"}}
    parsed = {}
    for spec in gates:
        name, gate = parse_gate(spec)
        if name in parsed:
            raise ValueError(f"duplicate gate {name!r}")
        parsed[name] = gate

    reasons = []
    for name in GATES:
        gate = parsed.get(name)
        if gate is None:
            parsed[name] = {"state": "UNVERIFIED", "tested_sha": "", "run_url": ""}
            if name not in optional:
                reasons.append(f"{name}: missing gate")
            continue
        if gate["tested_sha"] != target_sha:
            reasons.append(
                f"{name}: tested_sha {gate['tested_sha'] or '(missing)'} != target_sha {target_sha}"
            )
            continue
        allowed = accepted_states.get(name, {"PASS"})
        if gate["state"] not in allowed:
            reasons.append(
                f"{name}: {gate['state']} is not accepted; expected {' or '.join(sorted(allowed))}"
            )
        elif not gate["run_url"].strip():
            reasons.append(f"{name}: missing evidence reference")

    decision = release_decision if not reasons else "DO_NOT_RELEASE"
    recorded = utc_now()
    if approval and parsed["performance"]["state"] == "WARN":
        parsed["performance"]["rc_shared_node_approval"] = approval
    return {
        "schema": SCHEMA,
        "profile": profile,
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
    parser.add_argument("--profile", choices=sorted(PROFILES), default="stable")
    parser.add_argument("--gate", action="append", default=[])
    parser.add_argument("--out", default="")
    parser.add_argument(
        "--rc-shared-performance-approval",
        default="",
        help="maintainer approval reference for the measured shared-node RC performance WARN",
    )
    parser.add_argument(
        "--require-release",
        action="store_true",
        help="exit 1 when the recorded decision blocks publication",
    )
    args = parser.parse_args(argv)
    try:
        manifest = build_manifest(
            args.target_sha, args.gate, args.profile, args.rc_shared_performance_approval
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    text = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text)
    sys.stdout.write(text)
    return int(args.require_release and manifest["release_decision"] == "DO_NOT_RELEASE")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
