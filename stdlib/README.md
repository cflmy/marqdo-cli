# marqdo-stdlib (bundled first slice)

This directory is the official, offline `marqdo-stdlib` bundle consumed by
`std:<path>` imports. The CLI verifies `stdlib.json.digest` before loading any
module; a mismatch is a hard error.

- `format.mq.md` — value formatting (`显示`, list/map/table display, escaping)
- `text.mq.md` — compatibility UTF-8 encoding into `Memory`
- `text/utf8.mq.md` — `.mq.md` UTF-8 encoder and validating decoder
- `io.mq.md` — compatibility facade over `stream` and `console`
- `fs/file.mq.md` — bounded project-file read/write/copy over `resource.*`
- `io/stream.mq.md` — `写全部` / `写文本` with short-write and close checking
- `io/console.mq.md` — `读取行` / `打印` over `stdio/stdin` and `stdio/stdout`
- `demo.mq.md` — side-by-side comparison with builtin `打印`
- `data/json.mq.md` — recursive JSON parser and formatter (recoverable Result)
- `data/csv.mq.md` — quoted single-row CSV parser and formatter
- `data/table.mq.md` — value-returning table select/filter/sort/append/set/join/group/GFM
- `path.mq.md` — lexical join/basename/dirname/extension/normalize (no I/O)
- `regex.mq.md` — `^ $ . * + ? [...]` matcher and search (no groups)
- `net/http.mq.md` — HTTP/1.1 response parsing (headers/chunked) and request builder
- `tests/` — product self-tests for every public module
  (`console`, `demo`, `format`, `http`, `io`, `stream`, `table`, `text`, `utf8`)

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
