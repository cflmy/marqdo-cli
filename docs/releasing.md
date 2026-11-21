# Releasing Marqdo CLI

This document describes the release-candidate procedure. It does **not** grant
permission to tag or publish; that requires explicit owner approval and a green
required CI matrix on the exact commit.

## 1. Preconditions

- All applicable gates in
  [`STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md) are
  PASS, and `docs/evidence/ci/<SHA>/manifest.json` has
  `release_decision: RELEASE_CANDIDATE` with every gate's `tested_sha` equal to
  the release SHA. The manifest is produced by
  `tools/ci/release_evidence.py`; a missing, skipped, timed-out or
  SHA-mismatched gate is not PASS.
- **Language migration is closed through the release SHA.** The source language
  is [RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md); the
  release notes must state that this is a **breaking** change and that pre-RFC
  `.mq.md` sources are not executable (no `--legacy` path). The migration
  inventory is 100% classified, `stdlib/**/*.mq.md` and examples are migrated,
  and executable-source tests are green at `ffd1cc9`; re-run them for the actual
  release SHA. Platform and strict-lane evidence are still required before this
  is a releasable state; see [`STATUS.md`](STATUS.md) and
  [`migration/README.md`](migration/README.md).
- G-C is either PASS with a security-lane URL **and** a
  `strict-evidence.json` whose `tested_sha` is the release SHA, or the release
  explicitly says strict isolation is unavailable with owner approval. Never
  claim G-C from a local root/sudo run, a container, or a TCG/QEMU timing run.
  See [`security-gate.md`](security-gate.md) and [`security-vm.md`](security-vm.md).
- G4 performance is `MEASURED` on the fixed node with `--approved-node`, or
  explicitly `UNVERIFIED`/`WARN` with the reason recorded. Never copy a
  development-container or TCG number. See [`performance.md`](performance.md).
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
assume the examples are migrated to RFC-LANG-0002 and accepted by the frozen
binary; a failing example run is a release blocker, not a known migration
artifact:

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

## 7. Extension-ecosystem gate (RFC-EXT-0001)

Before a tag, in addition to §3 and §4:

1. `cargo build --release --locked -p marqdo-cli` and build
   `-p marqdo-provider-worker`; ship both binaries in the release archive for
   the same target. A provider request without the sibling helper must fail
   with `ext.worker_unavailable` — never fall back to `PATH`.
2. Run the external frozen-binary suite from
   `https://git.cflmy.cn/cflmy/marqdo-extension-conformance` against the exact
   release binaries and generate
   `docs/evidence/extension/<40-hex-source-SHA>/manifest.json` with
   `tools/ci/verify_extension_contracts.py`. The manifest must validate against
   `docs/evidence/extension/schema.json`, its `tested_sha` must equal the
   tagged commit, and every mandatory result must be `PASS` on the claimed
   platform. `UNSUPPORTED` is allowed only for a platform explicitly excluded
   from the support matrix in `COMPATIBILITY.md`.
3. `check`/`run`/`inspect`/Analysis share one prepared call binding
   (`spec/callable-resolution.md`); `check` never loads an extension binary;
   plugin exports reconcile exactly with static manifests; official stdlib
   aliases are verified metadata with byte-identical print output.
4. Existing gates remain mandatory and unchanged: G3 strict isolation evidence,
   G4 measured performance on the fixed node, native platform lanes, schema
   consumers and `tools/ci/release_evidence.py`. The extension evidence is an
   additional gate; it never substitutes for them, and local/container smoke is
   not a release claim.
5. If any extension gate is missing or mismatched, the release decision is
   **DO NOT RELEASE**. Tagging and asset publication still require explicit
   owner approval.
