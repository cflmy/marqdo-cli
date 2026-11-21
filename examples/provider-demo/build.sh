#!/usr/bin/env bash
# Build the example C resource provider and generate a host-specific
# provider/build/provider.json (schema v2) with the real artifact SHA-256.
#
# The tracked provider.json documents the manifest shape; the generated copy
# under build/ matches the machine that built it. A manifest digest mismatch is
# rejected at run time, never tolerated.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/build"
mkdir -p "$out/lib"
host="$(rustc -vV | sed -n 's/^host: //p')"
case "$host" in
  *windows*) echo "unsupported: the C ABI provider example targets Unix" >&2; exit 3 ;;
  *apple-darwin*) ext="dylib"; flags=(-dynamiclib -fPIC) ;;
  *) ext="so"; flags=(-shared -fPIC) ;;
esac
artifact="libsensor.$ext"
cc -std=c11 -O2 "${flags[@]}" -o "$out/lib/$artifact" "$here/src/sensor.c"
if command -v sha256sum >/dev/null 2>&1; then
  hash="$(sha256sum "$out/lib/$artifact" | cut -d' ' -f1)"
else
  hash="$(shasum -a 256 "$out/lib/$artifact" | cut -d' ' -f1)"
fi
cat > "$out/provider.json" <<JSON
{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [
    {
      "target": "$host",
      "path": "lib/$artifact",
      "sha256": "$hash"
    }
  ],
  "permissions_requested": []
}
JSON
echo "built $out/lib/$artifact for $host (sha256 $hash)"
echo "now run: marqdo run $here/demo.mq.md --provider $out/provider.json --allow-provider demo.sensor"
