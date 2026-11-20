# RFC-CM CM-A11 evidence: strict resource verification

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Source under test: `git archive 70d916d` (current HEAD)
Related: [ADR-0021](../adr/0021-linux-strict-isolation.md),
[ADR-0030](../adr/0030-print-compat-b-routing.md),
[ADR-0034](../adr/0034-tcp-resource-endpoint.md),
[STATUS §1](../STATUS.md).
Raw lane log: [`2026-11-20-strict-resource-cm-a11.log`](2026-11-20-strict-resource-cm-a11.log)

## Environment

Ubuntu 26.04 disposable QEMU/TCG VM, kernel 7.0.0-43-generic, ordinary user
`dev` (uid 1000), `systemd-detect-virt --container` = `none`, bubblewrap
0.11.1 with the packaged `bwrap-userns-restrict` profile. The dev container
cannot create namespaces, so this is VM-only evidence.

## What was verified

```text
./tests/security/run-lane.sh
  adversarial helper matrix: 7 passed, 0 failed, 0 ignored
  positive CLI contract: PASS
  strict language resources are console-only: PASS
  external-execution surfaces refused: PASS
  forged active marker rejected: PASS
  G-C security lane: PASS
```

The 7-test helper matrix covers: no host-file reads beyond `/project`, no
writes outside or into the project, no network, unprivileged namespaces, no
capability escalation, fd/process limits, and memory/CPU/output limits.

The positive CLI contract runs `hello` (Compat-C `打印` through the bundled
stdlib), `factorial --mode strict`, multi-module, and `--json`.

The strict-resource checks run `.mq.md` programs that call
`resource.open("file", "README.md", ["read"])` and
`resource.open("tcp", "127.0.0.1:9", ["read","write"])` under `--mode strict`;
each run must be non-zero and the diagnostic must contain `permission.denied`.
The `--allow-net` flag is also rejected at the strict boundary. The
bundled stdlib is bind-mounted read-only at `/run/marqdo-stdlib` and its digest
is still verified in the child; this grants no new OS capability.

## Policy verified in code/tests

* Strict permits only `stdio/stdout` with console grant.
* Strict refuses project-file, TCP and registered-provider resource kinds;
  `pure` refuses file/TCP/providers too.
* `--allow-read/--allow-write/--allow-foreign/--allow-plugin/--allow-net/
  --trace-file/--kernel/--trust-native-plugin` are rejected at the strict L1
  flag boundary.
* Strict permits the read-only bundled stdlib so `打印` works; file/network/
  process/device remain refused.
* TCP is project-only and strict refuses it (host tests), so the new TCP
  endpoint does not widen strict L1.

## Gate

Local container: fmt/clippy/426 tests PASS. VM: 7/7 adversarial + full lane
PASS. Log is checked in; this is a **local VM lane**, not a self-hosted GitHub
Actions security URL and not Windows/macOS evidence.

## Limits

* No remote CI URL for `70d916d`; G-C remains VM-verified, not CI-signed.
* Windows/macOS strict behavior is not exercised here.
* Strict L2 (Worker/Foreign/native plugin under real isolation) is still not
  implemented; L1 refuses those surfaces.
* TCP/HTTP/DNS under strict are refused rather than isolated.
