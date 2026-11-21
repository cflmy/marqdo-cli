#!/usr/bin/env bash
# Linux G-C security lane: run the adversarial strict-isolation matrix.
#
# This script must run as an ordinary user on a disposable Ubuntu VM or a
# trusted self-hosted runner. It refuses root because root-only success is not
# evidence that a normal user can use strict mode.
#
# Requirements: bwrap (Bubblewrap with its AppArmor profile), a C compiler,
# Rust toolchain, and a kernel/container policy that permits namespaces.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

started_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
evidence="${MARQDO_STRICT_EVIDENCE:-}"
finish_lane() {
    status=$?
    trap - EXIT
    set +e
    if [ -n "$evidence" ]; then
        outcome="FAIL"
        if [ "$status" -eq 0 ]; then
            outcome="PASS"
        fi
        if ! python3 tools/ci/strict-evidence.py \
            --repo "$repo" \
            --out "$evidence" \
            --outcome "$outcome" \
            --started "$started_utc" \
            --finished "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            --log "${MARQDO_STRICT_LANE_LOG:-}"; then
            echo "error: strict evidence generation failed; a green lane without evidence is not a gate" >&2
            if [ "$status" -eq 0 ]; then
                status=1
            fi
        fi
    fi
    exit "$status"
}
trap finish_lane EXIT

if [ "$(id -u)" -eq 0 ]; then
    echo "error: the security lane must run as a non-root user" >&2
    exit 1
fi
for tool in bwrap cc cargo python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool '$tool' is missing (a missing mechanism must fail G-C)" >&2
        exit 1
    fi
done

# A skipped security test must fail the gate, not pass silently.
export MARQDO_REQUIRE_STRICT_SECURITY=1
export CARGO_TERM_COLOR=never

echo "== adversarial helper matrix =="
cargo test -p marqdo-sandbox --test security_linux --locked -- --test-threads=1

echo "== build CLI =="
cargo build -p marqdo-cli --locked

target_dir="${CARGO_TARGET_DIR:-$repo/target}"
bin="$target_dir/debug/marqdo"
platform="$("$bin" inspect examples/hello.mq.md --platform --json)"
python3 - "$platform" <<'PY'
import json, sys
report = json.loads(sys.argv[1])["platform"]["strict"]
if report["state"] != "available":
    raise SystemExit(f"strict is not available: {report}")
if "bubblewrap" not in report.get("mechanism", ""):
    raise SystemExit(f"unexpected mechanism: {report}")
print(f"strict mechanism: {report['mechanism']}")
PY

echo "== positive CLI contract =="
"$bin" run examples/hello.mq.md --mode strict | grep -q "你好，Marqdo"
"$bin" call examples/factorial.mq.md 数学.阶乘 --arg n=5 --mode strict | grep -qx "120"
"$bin" run examples/multi-module/index.mq.md --mode strict >/dev/null
"$bin" run examples/hello.mq.md --mode strict --json | python3 -c \
  'import json,sys; d=json.load(sys.stdin); assert d["success"] is True'

echo "== strict language resources are console-only =="
mkdir -p .cache/security
cat > .cache/security/file-resource.mq.md <<'MQ'
## @exec main

`内存 = memory.new(1, 1)`
`resource.open("file", "README.md", ["read"])`
`return None`
MQ
set +e
"$bin" run .cache/security/file-resource.mq.md --mode strict >/dev/null 2>.cache/security/file-resource.err
code=$?
set -e
if [ "$code" -eq 0 ]; then
    echo "error: strict accepted a project file resource" >&2
    exit 1
fi
grep -q "permission.denied" .cache/security/file-resource.err || {
    echo "error: strict file-resource refusal had no permission.denied diagnostic" >&2
    cat .cache/security/file-resource.err >&2
    exit 1
}

cat > .cache/security/tcp-resource.mq.md <<'MQ'
## @exec main

`resource.open("tcp", "127.0.0.1:9", ["read", "write"])`
`return None`
MQ
set +e
"$bin" run .cache/security/tcp-resource.mq.md --mode strict >/dev/null 2>.cache/security/tcp-resource.err
code=$?
set -e
if [ "$code" -eq 0 ]; then
    echo "error: strict accepted a TCP resource" >&2
    exit 1
fi
grep -q "permission.denied" .cache/security/tcp-resource.err || {
    echo "error: strict TCP-resource refusal had no permission.denied diagnostic" >&2
    cat .cache/security/tcp-resource.err >&2
    exit 1
}

echo "== external-execution surfaces are refused =="
reject() {
    set +e
    "$bin" run examples/hello.mq.md --mode strict "$@" >/dev/null 2>&1
    code=$?
    set -e
    if [ "$code" -ne 2 ]; then
        echo "error: strict accepted unsupported flags '$*' (exit $code)" >&2
        exit 1
    fi
}
reject --allow-foreign
reject --allow-read 'data/**'
reject --allow-write 'data/**'
reject --allow-net 127.0.0.1
reject --trust-native-plugin --plugin examples/plugin-demo/build
reject --kernel examples/kernel-mock/kernel.json
reject --trace-file /tmp/marqdo-trace.json

echo "== @knowledge payloads stay inert under strict =="
rm -f .cache/security/knowledge-side-effect.txt
cat > .cache/security/knowledge-inert.mq.md <<'MQ'
## @exec main

`return None`

### @knowledge 说明

`resource.open("file", "knowledge-side-effect.txt", ["write"])`
MQ
"$bin" run .cache/security/knowledge-inert.mq.md --mode strict >/dev/null
if [ -e .cache/security/knowledge-side-effect.txt ]; then
    echo "error: a @knowledge payload produced a host side effect in strict mode" >&2
    exit 1
fi

echo "== forged active marker is rejected =="
set +e
MARQDO_STRICT_ACTIVE=1 "$bin" run examples/hello.mq.md --mode strict >/dev/null 2>&1
code=$?
set -e
if [ "$code" -ne 1 ]; then
    echo "error: forged strict marker was not rejected (exit $code)" >&2
    exit 1
fi

# The JSON contract is part of the gate: a rejected marker still produces
# exactly one parseable document with the stable code.
forged_json="$(mktemp)"
set +e
MARQDO_STRICT_ACTIVE=1 "$bin" run examples/hello.mq.md --mode strict --json >"$forged_json" 2>/dev/null
code=$?
set -e
if [ "$code" -ne 1 ]; then
    echo "error: forged strict marker JSON path had unexpected exit $code" >&2
    rm -f "$forged_json"
    exit 1
fi
python3 - "$forged_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    document = json.load(handle)
if document.get("success") is not False:
    raise SystemExit(f"forged marker JSON is not a failure: {document}")
if document.get("code") != "strict.identity_invalid":
    raise SystemExit(f"forged marker JSON code drifted: {document}")
PY
rm -f "$forged_json"

echo "G-C security lane: PASS"
if [ -n "$evidence" ]; then
    echo "strict evidence: $evidence"
fi
