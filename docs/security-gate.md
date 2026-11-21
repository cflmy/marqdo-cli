# Linux strict-isolation security gate (G-C) — evidence

Status: **PASSED on the current CI-fix series in a local disposable Ubuntu
VM; Linux strict is enabled only when the runtime probe succeeds. REMOTE G3
UNVERIFIED: no self-hosted security-lane CI URL exists yet.**

Current evidence: source SHA `5fcc111a304211dfbb3a4c85d72e0b3f4a2318be`,
ordinary user `dev` (uid 1000), QEMU/TCG guest, 7/7 adversarial tests plus the
strict `--json` and inert-`@knowledge` contracts — see
[`evidence/2026-11-21-strict-gc-5fcc111.md`](evidence/2026-11-21-strict-gc-5fcc111.md).
The older `8af7262` and `b6fc182` runs are retained below as history.
Snapshot: product `3.0.0-beta.1`, 2026-11-22. See ADR-0021 and ADR-0053.

> **Language-syntax status.** The lane was re-run on the RFC-LANG-0002 tree
> ([`architecture/RFC-LANG-0002-unified-semantics.md`](architecture/RFC-LANG-0002-unified-semantics.md))
> at `5fcc111`, including the strict `@knowledge`-inert check. The security
> facts are unchanged: `@exec` is a compile-time projection, not a capability
> grant; `@knowledge` produces no IR and therefore no host effect; resource,
> file, network, stdin/stdout, memory and budget checks remain at the
> `CapabilityHost` boundary, and the strict sandbox is unaffected. See
> [`STATUS.md`](STATUS.md) for the language gate and
> [`evidence/2026-11-21-semantics-v2-green.md`](evidence/2026-11-21-semantics-v2-green.md).
> The earlier `512b9f5`/`0d7e11b`/`990e984` runs are historical evidence;
> `5fcc111` is the native-green code SHA and also adds the CLI 8 MiB main
> thread, portable process fixture and the G3-only opt-in gate.

This report separates three things that are often conflated:

1. the **mechanism** (Bubblewrap namespaces + rlimits + supervisor),
2. the **runtime probe** that decides whether `strict` is available, and
3. the **adversarial evidence** that the boundary actually holds.

## 1. Why the earlier host could not pass

The first development host was a root-in-container environment:

```text
virt: container-other
capsh: Current: = (no effective capabilities), no-new-privs=1
unshare --user/--mount/--pid/--net -> Operation not permitted
bwrap --unshare-user --unshare-net -> No permissions to create a new namespace
cgroup2 -> ro,nosuid,nodev,noexec
```

That environment cannot verify G-C. Strict stayed `Unavailable` there; no
adapter was faked.

## 2. Disposable test VM

A QEMU/TCG Ubuntu 26.04 VM was created from the cloud image (no KVM available
on the outer host). Inside the VM:

```text
OS:               Ubuntu 26.04.1 LTS, kernel 7.0.0-43-generic x86_64
systemd-detect-virt --container: none
user:             dev (uid 1000), passwordless sudo for test setup only
cgroup2:          rw, nsdelegate, controllers: cpuset cpu io memory hugetlb pids rdma misc dmem
sysctl:           apparmor_restrict_unprivileged_userns = 1
bubblewrap:       0.11.1 with the packaged `bwrap-userns-restrict` profile
unprivileged:     bwrap --unshare-user --unshare-net --ro-bind / / true -> exit 0
```

Root can create namespaces, and the packaged Bubblewrap AppArmor profile
allows the **ordinary user** to run `bwrap` without disabling AppArmor
globally. The released strict mode runs as this ordinary user.

## 3. Implemented boundary (ADR-0021)

`marqdo run --mode strict`:

* probes Bubblewrap at runtime; failure => `Unavailable`, run exits 1, no
  project code executes;
* re-executes the CLI under `bwrap --unshare-all --as-pid-1 --die-with-parent
  --new-session --cap-drop ALL --clearenv`;
* mounts only runtime libraries, `/etc/ld.so.cache`, the program and the
  project tree read-only at `/project`; private `/proc`, minimal `/dev`,
  tmpfs `/tmp`;
* applies hard rlimits: CPU, 512 MiB address space, 64 fds, 64 processes,
  8 MiB stack, 0 core, output-byte file size;
* supervises wall-clock time and kills the process group;
* the child verifies `PID 1`, `NoNewPrivs: 1`, empty `CapEff`, a non-initial
  uid map and no non-loopback interface before running project code;
* rejects Foreign, native/worker plugins, kernels, read/write grants and trace
  files with exit 2 (strict L1 = pure language + native Math).

## 4. Adversarial evidence

The helper is `tests/security/escape_helper.c`, launched through the **same**
`marqdo-sandbox` supervisor, not through a separate ad-hoc bwrap command. The
lane is `tests/security/run-lane.sh` and the test is
`crates/marqdo-sandbox/tests/security_linux.rs`.

At `8af7262` the same lane was re-run from a clean `git archive` in the
disposable VM as `dev` uid 1000: 7/7 adversarial tests, positive CLI contract,
external-surface refusals, and forged-marker rejection all PASS. The command
now honours `CARGO_TARGET_DIR`; the parallel workspace run also passes all
seven security tests.

```text
7/7 tests PASS:
  strict_child_identity_is_unprivileged_and_namespaced
  strict_child_cannot_read_host_files_but_reads_the_project
  strict_child_cannot_write_outside_or_into_the_project
  strict_child_cannot_escape_via_proc_or_privileges
  strict_child_cannot_reach_the_network
  strict_supervisor_enforces_fd_and_process_limits
  strict_supervisor_enforces_memory_cpu_and_output_limits
```

Observed results inside the sandbox (ordinary user `dev`):

| Case | Observation |
|---|---|
| identity | `uid=1000`, `pid=1`, `NoNewPrivs: 1`, `CapEff: 0` |
| `/etc/shadow`, `/root`, `/home/dev` | `DENIED` |
| `/project/allowed.txt` | `OK` |
| write `/home/dev/...`, `/project/...` | `DENIED` (read-only) |
| write `/tmp` | allowed (private tmpfs; host unaffected) |
| `/proc/1/root/etc/shadow` | `DENIED`; no inherited fds above stdio |
| `setuid(0)`, `mount`, `unshare` | `DENIED` |
| IPv4 connect `1.1.1.1:80`, raw socket | `DENIED` |
| open `/dev/null` loop | `EMFILE` at <= 64 fds |
| fork loop | bounded below 200 (user process rlimit) |
| 768 MiB allocation | denied/killed by 512 MiB `RLIMIT_AS` |
| infinite CPU loop | stopped by wall-clock/CPU budget |
| 4 MiB stdout | stopped at the 64 KiB output limit |

CLI evidence (`dev`, VM):

```text
run examples/hello.mq.md --mode strict            -> 你好，Marqdo (exit 0)
call factorial 阶乘 --arg n=5 --mode strict       -> 120 (exit 0)
run examples/multi-module/index.mq.md --mode strict -> 12.566... (exit 0)
run --mode strict --json                          -> success=true
run --mode strict --allow-foreign                 -> exit 2
run --mode strict --trust-native-plugin ...       -> exit 2
run --mode strict --kernel ...                    -> exit 2
run --mode strict --trace-file ...                -> exit 2
MARQDO_STRICT_ACTIVE=1 run --mode strict          -> exit 1 (not PID 1)
hide /usr/bin/bwrap; run --mode strict            -> exit 1 (not installed)
MARQDO_STRICT_ACTIVE=1 run --mode strict --json   -> one JSON doc,
                                                     code=strict.identity_invalid
@knowledge payload with resource.open --mode strict -> no host side effect
```

### 4.1 Current evidence document

`5fcc111` was archived with `git archive` and transferred as an ordinary user:

```text
tested_sha:            5fcc111a304211dfbb3a4c85d72e0b3f4a2318be
source archive:        sha256:e13af3bbf65c576e57fde46ecf201925ad53fb70bf292b321baa96f154264fd2
strict-evidence.json:  sha256:38be28aa556b04214c4b3bd3cb26e2ad00ad49c1c3a579fc9e7059784ef5c5d0
lane log:              sha256:4f29201ab71b520ae3e0ea599bbff93a89cac8157fbdfc7037fac290477f83b7
base image:            sha256:7b90593f5532b49b7e3fe3d2743f0d1f0d822e5b07cebcd8e00c0df8069ecab9
guest:                 Ubuntu 26.04.1, kernel 7.0.0-43-generic, bwrap 0.11.1
user:                  dev uid 1000 (root=false), container=none
runner_kind:           disposable-qemu-tcg (not a performance node)
```

The machine-readable document is `marqdo.strict-evidence.v1`; it records
`tested_sha`, archive digest, uid/root, kernel, cgroup2, bwrap/rustc/cargo and
the outcome. A root PASS is rejected by the generator; this run is non-root.

## 5. Explicit non-goals / remaining work

* Strict **L2** (Worker/Foreign inheriting the sandbox) is not implemented;
  those flags are refused in strict. Native ABI is never allowed in strict.
* rlimits are used instead of cgroup-v2 accounting; the adapter does not claim
  cgroup enforcement.
* Filesystem TOCTOU is reduced by read-only mounts but not formally proven
  against a racing host user.
* Network is denied by a network namespace, not a seccomp allowlist.
* The disposable VM is reproducible from
  [`security-vm.md`](security-vm.md); that document also explains how to
  register it as an ephemeral self-hosted runner.
* **A remote security-lane CI URL and artifact are still required** before a
  release calls G-C CI-verified. `.github/workflows/security-linux.yml` now
  runs on trusted `master` pushes or an explicit `source_sha`, binds the
  checkout to the tested SHA, writes `strict-evidence.json` and uploads it.
  `MARQDO_REQUIRE_STRICT_SECURITY=1` makes missing mechanisms fail instead of
  skip.
* The current run is QEMU/TCG. It is evidence for **isolation correctness**,
  never for performance; see [`performance.md`](performance.md).
* The adversarial test file is opt-in: ordinary `cargo test --workspace` skips
  it, the lane sets `MARQDO_REQUIRE_STRICT_SECURITY=1`, and a developer may set
  `MARQDO_RUN_STRICT_SECURITY=1`. A skipped G0 test is not a G3 PASS.
