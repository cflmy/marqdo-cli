#!/usr/bin/env bash
# Build the example C plugin and generate a host-specific manifest.
#
# The artifact and generated manifest go to `build/`, which is gitignored. The
# tracked `plugin.yaml` is never rewritten by a test or build: it documents the
# manifest shape, while the generated one matches the machine that built it.
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
artifact="math_backend.$ext"
cc -std=c11 -O2 "${flags[@]}" -I include -o "$out/$artifact" "$here/math_backend.c"
if command -v sha256sum >/dev/null 2>&1; then
  hash="$(sha256sum "$out/$artifact" | cut -d' ' -f1)"
else
  hash="$(shasum -a 256 "$out/$artifact" | cut -d' ' -f1)"
fi
cat > "$out/plugin.yaml" <<YAML
plugin:
  name: math.backend
  version: 0.1.0
  abi: "^1.0"
  kind: native
  exports:
    - name: math_backend.add
      signature: "(\$a: Int, \$b: Int) -> Int"
  requires:
    - math.compute
  artifacts:
    - target: $host
      file: $artifact
      sha256: "$hash"
YAML
echo "built $out/$artifact for $host (sha256 $hash)"
echo "now run: marqdo plugin inspect $out"
