# RFC-CM CM-A10 evidence: deterministic offline stdlib package

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `e985da2` plus this step's changes.
Related: [ADR-0028](../adr/0028-bundled-stdlib-and-std-imports.md),
[STATUS §5](../STATUS.md).
Scope: packaging artifact for an optional future extraction; no CLI command.

## Artifact

`tools/pack-stdlib.py` builds `dist/marqdo-stdlib-0.1.0.tar.gz` from the
verified `stdlib/` tree with:

* sorted relative names, fixed mode/uid/gid/uname/gname and `mtime=0`;
* gzip `mtime=0` and an empty filename, so repeated builds are byte-identical;
* a JSON descriptor (`marqdo.stdlib-package`) with the tree digest, archive
  SHA-256 and file list; a `.sha256` sidecar is also written.

Descriptor: [`2026-11-20-stdlib-package.json`](2026-11-20-stdlib-package.json).

```text
version      0.1.0
tree digest  sha256:832940f35fe5651eabe4ac672e12e2d8f2bf4e649cc430e455425b4f2a983121
archive      sha256:94127ba3353bdef7430b2ef3c1914d99f9d7f48e4d72fa973eac7ca76c881394
files        11 (README, stdlib.json, *.mq.md)
```

Two consecutive builds compared byte-identically (`cmp`), and extracting the
archive and recomputing the custom tree digest reproduces the manifest digest.
`dist/` is gitignored, so the script and descriptor are the checked-in
reproducible inputs.

## Boundary

This does **not** publish a separate `marqdo-stdlib` repository. ADR-0039
intentionally keeps the stdlib in this repository for now; the archive is
maintained as extraction-ready output for a later, trigger-driven split.

The in-repo CLI still uses the bundled `stdlib/` tree with digest verification
and the frozen-binary closure test (ADR-0028); this archive is not loaded by
the CLI.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
python3 tools/pack-stdlib.py > /tmp/stdlib-package.json
sha256sum dist/marqdo-stdlib-0.1.0.tar.gz
```
