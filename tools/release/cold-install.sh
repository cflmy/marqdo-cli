#!/usr/bin/env bash
# Exercise only the shipped Linux archive on the supported glibc baseline.
set -euo pipefail

if [[ $# != 2 || ! $2 =~ ^[a-f0-9]{40}$ ]]; then
    echo "usage: $0 <archive.tar.gz> <source-sha>" >&2
    exit 2
fi
archive="$(realpath "$1")"
source_sha="$2"
name="$(basename "$archive")"
if [[ ! $name =~ ^marqdo-(.+)-x86_64-unknown-linux-gnu\.tar\.gz$ ]]; then
    echo "unexpected archive name: $name" >&2
    exit 2
fi
version="${BASH_REMATCH[1]}"
(
    cd "$(dirname "$archive")"
    sha256sum --check --status "$name.sha256"
)

docker run --rm --network none --user 65534:65534 \
    --mount "type=bind,source=$archive,target=/archive.tar.gz,readonly" \
    ubuntu:24.04 sh -eu -c '
    test "$(getconf GNU_LIBC_VERSION)" = "glibc 2.39"
    if command -v cargo; then
        echo "unexpected Cargo binary in the runtime container" >&2
        exit 1
    fi
    mkdir /tmp/marqdo
    tar -xzf /archive.tar.gz -C /tmp/marqdo
    cd "/tmp/marqdo/marqdo-$1-x86_64-unknown-linux-gnu"
    test "$(cat SOURCE_SHA)" = "$2"
    test "$(./marqdo --version)" = "marqdo $1"
    ./marqdo --help >/dev/null
    test -x ./marqdo-provider-worker
    ./marqdo run examples/hello.mq.md | grep -F "你好，Marqdo"
    ./marqdo check examples/multi-module/index.mq.md --emit /tmp/analysis.json
    ./marqdo call examples/math-library-demo.mq.md main
    test "$(./marqdo call examples/factorial.mq.md 阶乘 --arg n=5)" = 120
    ./marqdo run tests/consumers/fixtures/execution-events/basic.mq.md \
        --event-stream jsonl >/tmp/events.jsonl
    tail -n 1 /tmp/events.jsonl | grep -Eq "\"kind\"[[:space:]]*:[[:space:]]*\"run.completed\""
    mv stdlib stdlib.hidden
    if ./marqdo run examples/hello.mq.md >/tmp/missing-stdlib.log 2>&1; then
        echo "run succeeded without the shipped stdlib" >&2
        exit 1
    fi
    grep -E "stdlib|symbol\.unknown|打印" /tmp/missing-stdlib.log
' sh "$version" "$source_sha"
