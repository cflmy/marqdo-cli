#!/usr/bin/env bash
# Build the multi-export example and generate a host-specific manifest under
# `build/` (gitignored). The tracked manifest is never rewritten.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
cd "$repo"

out="$here/build"
mkdir -p "$out"
host="$(rustc -vV | sed -n 's/^host: //p')"
case "$host" in
  *apple-darwin*) ext="dylib"; flags=(-dynamiclib -fPIC) ;;
  *windows*) ext="dll"; flags=(-shared) ;;
  *) ext="so"; flags=(-shared -fPIC) ;;
esac
artifact="solver.$ext"
cc -std=c11 -O2 "${flags[@]}" -I include -DMQ_SOLVER_MULTI_EXPORT \
  -o "$out/$artifact" "$repo/examples/plugin-demo/solver.c"
if command -v sha256sum >/dev/null 2>&1; then
  hash="$(sha256sum "$out/$artifact" | cut -d' ' -f1)"
else
  hash="$(shasum -a 256 "$out/$artifact" | cut -d' ' -f1)"
fi
cat > "$out/plugin.yaml" <<YAML
plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.add
      signature: "(\$a: Int, \$b: Int) -> Int"
    - name: solver.neg
      signature: "(\$a: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: $host
      file: $artifact
      sha256: "$hash"
YAML
echo "built $out/$artifact for $host (sha256 $hash)"
echo "now run: marqdo plugin inspect $out"
