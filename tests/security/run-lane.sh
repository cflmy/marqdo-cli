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

if [ "$(id -u)" -eq 0 ]; then
    echo "error: the security lane must run as a non-root user" >&2
    exit 1
fi
for tool in bwrap cc cargo; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool '$tool' is missing (a missing mechanism must fail G-C)" >&2
        exit 1
    fi
done

# A skipped security test must fail the gate, not pass silently.
export MARQDO_REQUIRE_STRICT_SECURITY=1
export CARGO_TERM_COLOR=never

echo "== adversarial helper matrix =="
cargo test -p marqdo-sandbox --test security_linux -- --test-threads=1

echo "== build CLI =="
cargo build -p marqdo-cli

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
reject --trust-native-plugin --plugin examples/plugin-demo/build
reject --kernel examples/kernel-mock/kernel.json
reject --trace-file /tmp/marqdo-trace.json

echo "== forged active marker is rejected =="
set +e
MARQDO_STRICT_ACTIVE=1 "$bin" run examples/hello.mq.md --mode strict >/dev/null 2>&1
code=$?
set -e
if [ "$code" -ne 1 ]; then
    echo "error: forged strict marker was not rejected (exit $code)" >&2
    exit 1
fi

echo "G-C security lane: PASS"
