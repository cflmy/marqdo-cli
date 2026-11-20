#!/usr/bin/env bash
# Build the example C plugin and refresh the sha256 in plugin.yaml.
#
# The artifact is intentionally NOT committed: installing/inspecting a package
# never implies trusting or loading its native code, and the manifest must
# describe the artifact you actually build on your machine.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
cd "$repo"
cc -shared -fPIC -I include -o "$here/solver.so" "$here/solver.c"
hash="$(sha256sum "$here/solver.so" | cut -d' ' -f1)"
python3 - "$here/plugin.yaml" "$hash" <<'PY'
import sys, re, pathlib
path, digest = sys.argv[1], sys.argv[2]
p = pathlib.Path(path)
text = p.read_text()
text = re.sub(r'(?m)^(\s*sha256:\s*")[0-9a-fA-F]{64}(")', r'\g<1>' + digest + r'\g<2>', text)
p.write_text(text)
PY
echo "built $here/solver.so (sha256 $hash)"
echo "now run: marqdo plugin inspect $here"
