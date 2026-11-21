# CI evidence tools

## `strict-evidence.py`

Writes `marqdo.strict-evidence.v1`, called by `tests/security/run-lane.sh` when
`MARQDO_STRICT_EVIDENCE` is set and by the self-hosted security workflow. It
records only measured or explicitly supplied facts:

* `tested_sha` and whether it came from `git rev-parse` or an operator env;
* source archive SHA256 (and whether it matched the expected digest);
* kernel/uname/cgroup2/container, `uid`/root, bwrap/rustc/cargo/cc versions;
* runner kind, image digest, run URL, start/finish and outcome.

A root-only `PASS` is rejected. `PASS` from a container or a persistent
self-hosted runner with production credentials is not valid G-C evidence; see
[`docs/security-vm.md`](../../docs/security-vm.md).

## `release_evidence.py`

Aggregates per-gate results into `marqdo.release-gates.v1`:

```bash
python3 tools/ci/release_evidence.py \
  --target-sha "$SHA" \
  --gate "linux=PASS:$SHA:https://ci.example/run/1" \
  --gate "windows=PASS:$SHA:https://ci.example/run/2" \
  --gate "macos=PASS:$SHA:https://ci.example/run/3" \
  --gate "strict_vm=PASS:$SHA:https://ci.example/run/4" \
  --gate "performance=WARN:$SHA:https://ci.example/run/5" \
  --out "docs/evidence/ci/$SHA/manifest.json"
```

A missing gate becomes `UNVERIFIED`. Any non-PASS required gate, a
`tested_sha` different from `target_sha`, an unknown gate name or a malformed
SHA forces `DO_NOT_RELEASE`. `performance` may be `PASS` or `WARN`.

Unit tests: `python3 -m unittest discover -s tools/ci -p 'test_*.py'`.
