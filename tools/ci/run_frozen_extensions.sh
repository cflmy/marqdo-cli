#!/usr/bin/env bash
# Run the mandatory Linux lane with only the installed archive and pinned fixtures.
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "usage: $0 ARCHIVE SOURCE_SHA OUTPUT_MANIFEST" >&2
  exit 2
fi

archive="$1"
source_sha="$2"
manifest="$3"
root="$(cd "$(dirname "$0")/../.." && pwd)"
fixture_remote=https://git.cflmy.cn/cflmy/marqdo-extension-conformance.git
fixture_sha=015ff8ac15417b7ebbdb31ce2be2dab5362a1eba

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
git init -q "$work/fixtures"
git -C "$work/fixtures" fetch --no-tags --depth=1 "$fixture_remote" "$fixture_sha"
git -C "$work/fixtures" checkout -q --detach "$fixture_sha"

mkdir "$work/install"
tar -xzf "$archive" -C "$work/install"
installations=("$work"/install/*)
if [[ ${#installations[@]} != 1 || ! -d "${installations[0]}" ]]; then
  echo "expected one installation directory in the release archive" >&2
  exit 1
fi
install="${installations[0]}"
test "$(cat "$install/SOURCE_SHA")" = "$source_sha"
test -x "$install/marqdo"
test -x "$install/marqdo-provider-worker"

python3 "$root/tools/ci/verify_extension_contracts.py" \
  --source-repo "$root" --tested-sha "$source_sha" \
  --cli "$install/marqdo" --release-info "$install/RELEASE_INFO.json" \
  --fixtures "$work/fixtures" --out "$manifest"
