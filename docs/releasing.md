# Releasing Marqdo CLI

This document describes the release-candidate procedure. It does **not** grant
permission to tag or publish; that requires explicit owner approval and a green
required CI matrix on the exact commit.

## 1. Preconditions

- All applicable gates in
  [`NEXT_RELEASE_CI_CONVERGENCE.md`](NEXT_RELEASE_CI_CONVERGENCE.md) §7 are
  PASS with a CI URL for the same SHA.
- **Language migration is closed for the release SHA.** The source language is
  [RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md); the
  release notes must state that this is a **breaking** change and that
  pre-RFC `.mq.md` sources are not executable (no `--legacy` path). The
  migration inventory must be 100% classified, `stdlib/**/*.mq.md` and examples
  migrated, and executable-source tests green on the migrated tree. Until then
  this document describes a candidate only, never a releasable state; see
  [`STATUS.md`](STATUS.md) and [`migration/README.md`](migration/README.md).
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
compatibility matrix, specs, examples and the public protocol consumers. Build
it from the repository root:

```bash
version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
target="$(rustc -vV | sed -n 's/^host: //p')"
stage="dist/marqdo-${version}-${target}"
rm -rf "$stage"
mkdir -p "$stage"
install -m 0755 target/release/marqdo "$stage/"
cp LICENSE* CHANGELOG.md COMPATIBILITY.md README.md "$stage/"
cp -r spec examples "$stage/"
mkdir -p "$stage/tests"
cp -r tests/consumers "$stage/tests/"
tar -C dist -czf "dist/marqdo-${version}-${target}.tar.gz" "$(basename "$stage")"
sha256sum "dist/marqdo-${version}-${target}.tar.gz" \
  | tee "dist/marqdo-${version}-${target}.tar.gz.sha256"
```

If the repository has no `LICENSE*` files at release time, do not silently
skip the license requirement: add them before shipping.

## 4. Clean-install smoke

Run in a fresh directory (not the source tree). The example commands below
assume the examples have been migrated to RFC-LANG-0002 and are accepted by the
frozen binary; a failing example run on an un-migrated tree is not a release
signal:

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
python3 tests/consumers/analysis_consumer.py analysis.json
python3 tests/consumers/trace_consumer.py analysis.json trace.json
python3 tests/consumers/validate_protocols.py analysis.json trace.json \
  examples/tool-unknown-name/tool.json
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

The current `0.1.0` candidate at `8af7262` is **not** releasable: it has local
Linux/VM evidence and a checksummed Linux RC, but no same-SHA GitHub CI or
self-hosted G-C URL. See
[`evidence/RELEASE_CANDIDATE_0.1.0_8af7262.md`](evidence/RELEASE_CANDIDATE_0.1.0_8af7262.md).

## 6. Rollback

- If an asset fails verification: delete the release asset and keep the tag
  only if source/tag integrity is unaffected; otherwise publish a revised
  patch release and mark the old one withdrawn.
- If a platform regresses: remove that platform from the supported matrix in
  `COMPATIBILITY.md` and the release notes, fix, and re-run the gate. Never
  delete the failing test.
- If the security gate regresses: set strict back to `Unavailable`, publish a
  security note, and do not claim isolation until the lane is green again.
