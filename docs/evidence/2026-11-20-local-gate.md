# Local gate evidence — 2026-11-20

Code commit tested: `2cf9ecedd590f57cd8a816d26dbe19417ddfed81`.
Workspace version: `0.1.0` (no tag). Host: Ubuntu 26.04 container,
Rust 1.100.0, x86_64.

This is **local** evidence. It is not a GitHub Actions URL and does not replace
the native Windows/macOS lanes. See `COMPATIBILITY.md` for the honest matrix.

## Commands and results

```text
cargo fmt --all -- --check                             PASS
cargo clippy --workspace --all-targets -- -D warnings  PASS
cargo test --workspace                                 PASS (46 test binaries)
cargo build --release --locked -p marqdo-cli           PASS
```

Notable suites in the run:

```text
conformance_ast                 4 tests PASS (LF canonical + CRLF byte fixture)
integration_cli                23 tests PASS
plugin_worker                   4 tests PASS
marqdo-foreign provider         8 tests PASS
analysis_trace / tool_protocol  PASS
package_env                     PASS
```

## Windows checkout reproduction

A fresh clone with `core.autocrlf=true` was made from this tree:

```text
golden_has_CR False
crlf_fixture_has_CRLF True
cargo test -p marqdo-cli --test conformance_ast -> 4 passed
```

Converting one golden to CRLF before the fix reproduced the original
"0 line(s) differ" symptom; the new diagnostic reported the first differing
byte and classified it as line endings only.

## Release candidate and clean-install smoke

The release binary was packaged with specs, examples, licenses, changelog and
compatibility matrix, then extracted into a fresh temporary directory. The
SHA-256 file verified before extraction. Executed from the extracted tree:

```text
marqdo --version                     marqdo 0.1.0
marqdo --help                        exit 0
check examples/factorial.mq.md       exit 0
run examples/hello.mq.md             你好，Marqdo
call factorial 阶乘 --arg n=5        120
check multi-module --emit analysis   exit 0
analysis_consumer.py                 exit 0 (functions=2)
run multi-module --trace-file        exit 0
trace_consumer.py                    exit 0 (events=7)
run packages-multi-version           exit 0
run python-foreign --allow-foreign   28.274333882308138
run hello --mode strict              exit 1, refusal message
tool install/run/remove              exit 0
```

## Environment limits observed

- The process is root inside a container with no effective capabilities
  (`capsh`: `Current: =`) and `no-new-privs=1`.
- `setpriv --reuid=65534` fails (`setresuid: Operation not permitted`), so a
  true non-root run could not be performed on this host.
- `unshare` (user/mount/PID/net) and `bwrap` fail with `EPERM`; cgroup2 is
  mounted read-only.
- Therefore G-C (strict OS isolation) is **not passed**; strict remains
  fail-closed. `docs/security-gate.md` records the full probe.
