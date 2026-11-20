#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
cd "$repo"
cc -std=c11 -O2 -shared -fPIC -I include -DMQ_SOLVER_MULTI_EXPORT \
  -o "$here/solver.so" examples/plugin-demo/solver.c
hash="$(sha256sum "$here/solver.so" | cut -d' ' -f1)"
python3 - "$here/plugin.yaml" "$hash" <<'PY'
import pathlib, re, sys
path, digest = sys.argv[1], sys.argv[2]
p = pathlib.Path(path)
text = p.read_text()
text = re.sub(r'(?m)^(\s*sha256:\s*")[0-9a-fA-F]{64}(")', r'\g<1>' + digest + r'\g<2>', text)
p.write_text(text)
PY
echo "built $here/solver.so"
