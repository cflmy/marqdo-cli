# Releasing Marqdo CLI

This document describes the release-candidate procedure. It does **not** grant
permission to tag or publish; that requires explicit owner approval and a green
required CI matrix on the exact commit.

## 1. Preconditions

- All applicable G-B gates in
  [`STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md) §16.1
  are PASS with a CI URL for the same SHA.
- G-C is either PASS with a security-lane URL, or the release explicitly says
  strict isolation is unavailable. Never claim G-C from local root/sudo runs.
- `git status --porcelain` is clean apart from intended release files.
- `Cargo.lock` is committed and the build uses `--locked`.

## 2. Build verification

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --locked -p marqdo-cli
```

## 3. Artifact creation

The artifact is a tarball containing the binary, license, README, changelog,
compatibility matrix and specs. Build it from the repository root:

```bash
version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
target="$(rustc -vV | sed -n 's/^host: //p')"
stage="dist/marqdo-${version}-${target}"
rm -rf "$stage"
mkdir -p "$stage"
install -m 0755 target/release/marqdo "$stage/"
cp LICENSE* CHANGELOG.md COMPATIBILITY.md README.md "$stage/"
cp -r spec "$stage/"
tar -C dist -czf "dist/marqdo-${version}-${target}.tar.gz" "$(basename "$stage")"
sha256sum "dist/marqdo-${version}-${target}.tar.gz" \
  | tee "dist/marqdo-${version}-${target}.tar.gz.sha256"
```

If the repository has no `LICENSE*` files at release time, do not silently
skip the license requirement: add them before shipping.

## 4. Clean-install smoke

Run in a fresh directory (not the source tree):

```bash
tmp="$(mktemp -d)"
tar -C "$tmp" -xzf dist/marqdo-*.tar.gz
cd "$tmp"/marqdo-*
./marqdo --version
./marqdo --help >/dev/null
./marqdo check examples/multi-module/index.mq.md
./marqdo run examples/hello.mq.md
./marqdo call examples/factorial.mq.md 阶乘 --arg n=5
./marqdo check examples/multi-module/index.mq.md --refs --emit analysis.json
./marqdo run examples/multi-module/index.mq.md --trace-file trace.json
python3 /path/to/tests/consumers/analysis_consumer.py analysis.json
python3 /path/to/tests/consumers/trace_consumer.py analysis.json trace.json
```

Also verify the SHA-256 file before extraction:

```bash
sha256sum -c dist/marqdo-*.tar.gz.sha256
```

## 5. Tagging and publication

Only after the above and the remote CI matrix are green on the release commit:

```bash
git fetch origin --tags
test "$(git rev-parse HEAD)" = "$(git rev-parse origin/master)"
git tag -s "v${version}" -m "Marqdo CLI v${version}"
git push origin "v${version}"
# upload the tarball and .sha256 to the release
```

Do not force-push, do not tag a red commit, and do not upload macOS/Windows
binaries unless those platform lanes actually passed.

## 6. Rollback

- If an asset fails verification: delete the release asset and keep the tag
  only if source/tag integrity is unaffected; otherwise publish a revised
  patch release and mark the old one withdrawn.
- If a platform regresses: remove that platform from the supported matrix in
  `COMPATIBILITY.md` and the release notes, fix, and re-run the gate. Never
  delete the failing test.
- If the security gate regresses: set strict back to `Unavailable`, publish a
  security note, and do not claim isolation until the lane is green again.
