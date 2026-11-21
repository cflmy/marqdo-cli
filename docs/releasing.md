# Releasing Marqdo CLI (`3.0.0-beta.1` and later)

This document is the release-candidate procedure. It does **not** grant
permission to tag or publish: a tag requires the gates below to be green on the
exact commit and an explicit maintainer sign-off recorded in the release notes.

Two profiles are deliberately separate (see
[`MARQDO_3_0_PRERELEASE_PLAN.md`](MARQDO_3_0_PRERELEASE_PLAN.md) §3):

* **beta** — a prerelease (`-beta.N`) on the platform(s) the project can really
  test and fix. Known non-security gaps are allowed only when explicitly
  recorded. `3.0.0-beta.1` claims **Linux x86_64 (glibc)** only.
* **stable** — `3.0.0`/LTS. Not covered by beta exceptions; see
  [`STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md)
  (historical roadmap input) and the public support matrix.

The binary release pipeline uses the Gitea remote
[`git.cflmy.cn/cflmy/marqdo-cli`](https://git.cflmy.cn/cflmy/marqdo-cli) and
Woodpecker [`ci.cflmy.cn`](https://ci.cflmy.cn). `.woodpecker/main.yml` runs on
`push`/`pull_request`, so a tag push alone does not build release assets; build
the archive from the tagged commit on Linux x86_64 and attach it to the Gitea
release. Do not publish binaries built from a different checkout.

## 1. Beta preconditions (non-waivable)

Freeze one **release SHA** first; any later source change creates a new SHA and
requires re-running every gate. On that SHA:

1. `git status --porcelain` is clean; `Cargo.lock` is committed; all builds use
   `--locked`.
2. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace --locked` PASS.
3. `cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker`
   PASS; both binaries carry the same target triple and source SHA.
4. The same-SHA Woodpecker Linux pipeline PASS (fmt, clippy, tests, Windows
   cross-target compile gate, release build).
5. **Frozen-binary external conformance:** build the release binary first, then
   run the independent fixtures from
   [`marqdo-extension-conformance`](https://git.cflmy.cn/cflmy/marqdo-extension-conformance)
   against it and write
   `docs/evidence/extension/<40-hex-release-sha>/manifest.json` with
   `tools/ci/verify_extension_contracts.py`. The CLI binary hash must not move
   during the suite. All seven mandatory result categories PASS.
6. **Strict policy (beta gate 5):** either same-SHA `strict-evidence.json` from
   the disposable VM (`outcome: PASS`, non-root uid) for Linux strict L1, or an
   explicitly tested **fail-closed `strict: unsupported`** policy in the
   release notes. A local root/container-only PASS is never promoted to a
   remote claim; if the evidence is local-VM-only, say exactly that.
7. **G4 performance (beta gate 6):** correctness-verified `MEASURED` report on
   the fixed Woodpecker agent. If the fixed node is still a shared/powersave
   host, the maintainer may approve a **Beta-only WARN** and must publish the
   environment and limits; the 9 correctness workloads must still pass. Never
   report TCG/QEMU or development-container timings as G4.
8. `README`, `CHANGELOG`, `COMPATIBILITY`, `docs/STATUS`, this file,
   `spec/tool-v1.md`, `spec/plugin-abi.md` and the developer docs are on the
   same fact baseline. No "implemented but says Proposed", no "released but
   says DO NOT RELEASE", no product/protocol/package version confusion.
9. Every shipped binary was actually built and smoke-tested on its claimed
   platform. The archive records SHA256, `SOURCE_SHA`, toolchain and known
   limitations; checksums are integrity, not publisher authentication.

Windows/macOS native lanes, strict L2, uv/npm environment creation, registry,
JIT and built-in Knowledge/Proof/View are **not** beta gates; they are listed as
unsupported/experimental in
[`developers/release-limits.md`](developers/release-limits.md).

## 2. Build and local gate

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker
```

Record the toolchain (`rustc -vV`, `cargo --version`), the release SHA and the
binary SHA256 values; `tools/release/package.py` writes them into
`RELEASE_INFO.json`.

## 3. Release archive

The archive is built deterministically from a clean tree at `SOURCE_SHA`:

```bash
version="$(cargo metadata --no-deps --format-version 1 --offline \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="marqdo-cli"))')"
sha="$(git rev-parse HEAD)"
python3 tools/release/package.py --version "$version" --source-sha "$sha"
# dist/marqdo-<version>-x86_64-unknown-linux-gnu.tar.gz(+.sha256)
```

Contents: `marqdo` **and** `marqdo-provider-worker` in the same directory,
`LICENSE-*`, `README.md`, `CHANGELOG.md`, `COMPATIBILITY.md`,
`RELEASE_INFO.json`, `SOURCE_SHA`, `KNOWN_LIMITATIONS.md`, `spec/`,
`docs/developers/`, `examples/` (with host-built plugin/provider artifacts so
the examples run without a compiler) and `tests/consumers/`.

Do not publish an archive with only the CLI, with a mismatched worker target, or
with a checksum that does not match the bytes.

## 4. Clean-install smoke (from the archive only)

Run outside the source tree. The script below extracts the archive into a fresh
directory and exercises only the shipped binaries:

```bash
python3 tools/release/smoke.py \
  --archive dist/marqdo-*.tar.gz
```

It verifies the `.sha256`, `RELEASE_INFO.json`/`SOURCE_SHA`, offline
`--version`/`--help`, `check`/`run`/`call`, lock/sync, Analysis v2 and Trace v2
consumers, Tool install/dispatch, native + worker plugin transports, a foreign
kernel, Resource Provider v2 through the shipped worker (positive,
unauthorized and missing-helper fail-closed), and strict mode when the host
actually supports it. A failure is a release blocker.

## 5. Release evidence manifest

```bash
sha="$(git rev-parse HEAD)"
python3 tools/ci/release_evidence.py --profile beta --target-sha "$sha" \
  --gate linux=PASS:$sha:<woodpecker-pipeline-url> \
  --gate strict_vm=PASS:$sha:docs/evidence/<strict-doc> \
  --gate extension=PASS:$sha:docs/evidence/extension/$sha/manifest.json \
  --gate clean_install=PASS:$sha:docs/evidence/<smoke-doc> \
  --gate version_compat=PASS:$sha:<version-test-log-or-pipeline> \
  --gate windows=UNSUPPORTED:$sha:linux-only-beta-support-matrix \
  --gate macos=UNSUPPORTED:$sha:linux-only-beta-support-matrix \
  --gate performance=WARN:$sha:docs/evidence/<g4-doc> \
  --out "docs/evidence/ci/$sha/manifest.json"
```

`BETA_RELEASE_CANDIDATE` is the only acceptable decision for a beta tag. A
missing, skipped, failed or SHA-mismatched gate is `DO_NOT_RELEASE`; never
hand-edit a manifest to PASS. Stable releases use the default `--profile
stable` and additionally require Windows/macOS PASS and a strict PASS.

## 6. Tagging and publication (owner approval required)

Only after all gates above are green on the frozen SHA:

```bash
git fetch origin --tags
test "$(git rev-parse HEAD)" = "$(git rev-parse origin/master)"
git tag -a "v${version}" -m "Marqdo CLI v${version}" "$sha"
git push origin "v${version}"
```

Then create the Gitea release for the tag with `prerelease: true`, attach the
tarball and its `.sha256`, and include the release notes. The notes must state:

* beta / non-LTS status and the Linux x86_64-only support claim;
* the breaking RFC-LANG-0002 language change and no legacy path;
* the strict policy (same-SHA evidence, or `strict: unsupported` fail-closed);
* the G4 Beta-only WARN environment and limits when applicable;
* independently versioned protocols and stdlib/package versions;
* the checksum-integrity-not-signature caveat and the known-limitations file;
* the exact source SHA and links to the evidence manifest.

Example Gitea API shape (token with repository release permission; never commit
the token):

```bash
curl -X POST -H "Authorization: token $MARQDO_GIT_TOKEN" \
  -H 'Content-Type: application/json' \
  https://git.cflmy.cn/api/v1/repos/cflmy/marqdo-cli/releases \
  -d '{"tag_name":"v3.0.0-beta.1","name":"Marqdo CLI 3.0.0-beta.1",
       "prerelease":true,"draft":false,"body":"..."}'
curl -X POST -H "Authorization: token $MARQDO_GIT_TOKEN" \
  -F "attachment=@dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz" \
  https://git.cflmy.cn/api/v1/repos/cflmy/marqdo-cli/releases/<id>/assets
```

The tag points at the tested SHA. Evidence files and the final STATUS update
may land in a **post-release docs commit** that references the tagged SHA, so
that the evidence never has to claim a SHA it cannot contain. Do not force-push,
do not tag a red commit, and do not upload macOS/Windows binaries unless those
platform lanes actually passed.

## 7. Rollback

- Missing/mismatched gate discovered after tagging: mark the release withdrawn,
  delete the assets, keep the tag only if source/tag integrity is unaffected,
  and ship a corrected prerelease (for example `3.0.0-beta.2`). Never delete a
  failing test.
- Platform regression: remove that platform from `COMPATIBILITY.md`, the
  release notes and the archive, then fix and re-run the gate.
- Security regression: set strict back to fail-closed/unsupported, publish a
  security note, and do not claim isolation until fresh same-SHA evidence
  exists.
- Performance regression: keep the previous fixed-node baseline, record the
  new WARN/FAIL truthfully, and do not substitute another machine.

## 8. Extension-ecosystem gate (additional, never a substitute)

The external conformance gate is mandatory for every tag. It proves that an
unchanged CLI binary accepts independent `.mq.md` packages, plugins, foreign
kernels, resource providers and CLI tools, and that the negative paths
(unauthorized, wrong digest, wrong target, wrong schema, tampered artifact)
fail closed. Generate the manifest with
`tools/ci/verify_extension_contracts.py`; its `tested_sha` must equal the
tagged commit and its CLI binary hash must be the shipped binary. See
[`evidence/extension/README.md`](evidence/extension/README.md).

## 9. Stable releases

The beta exceptions in §1 (platform scope, G4 WARN, strict-unsupported policy)
do **not** apply to `3.0.0` or LTS. A stable release requires every committed
feature on the supported platforms to have same-SHA security, compatibility and
performance evidence, a public change/migration policy, and a real user-feedback
cycle. Until then the product stays a prerelease.
