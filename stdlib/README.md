# marqdo-stdlib (bundled first slice)

This directory is the official, offline `marqdo-stdlib` bundle consumed by
`std:<path>` imports. The CLI verifies `stdlib.json.digest` before loading any
module; a mismatch is a hard error.

- `format.mq.md` — value formatting (`显示`, list/map/table display, escaping)
- `text.mq.md` — UTF-8 encoding into `Memory`
- `io.mq.md` — `写文本` / `写值` through `resource.write(stdout)`
- `demo.mq.md` — side-by-side comparison with builtin `打印`
- `data/table.mq.md` — value-returning table select/filter/sort/append/set/join/group/GFM
- `net/http.mq.md` — HTTP/1.1 response parsing (headers/chunked) and request builder
- `tests/` — bundle self-tests

The digest is a SHA-256 over sorted `*.mq.md` relative paths, a NUL, file
bytes, and a NUL. It excludes `stdlib.json` and non-`.mq.md` files. After any
module change, regenerate it with:

```bash
python3 - stdlib <<'PY'
import hashlib, json, sys
from pathlib import Path
root=Path(sys.argv[1])
h=hashlib.sha256()
for p in sorted(root.rglob('*.mq.md')):
    h.update(p.relative_to(root).as_posix().encode()); h.update(b'\0')
    h.update(p.read_bytes()); h.update(b'\0')
manifest=json.loads((root/'stdlib.json').read_text())
manifest['digest']='sha256:'+h.hexdigest()
(root/'stdlib.json').write_text(json.dumps(manifest, indent=2)+'\n')
PY
```

The version/digest are part of each stdlib module's package instance id, so a
different bundle cannot silently masquerade as the one a project was checked
against.
