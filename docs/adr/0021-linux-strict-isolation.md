# ADR-0021: Linux strict isolation via Bubblewrap, namespaces and rlimits

Status: **Accepted**
Date: 2026-11-20
Relates to: ADR-0004, ADR-0019, `spec/security.md`, stable-release plan §10
Security evidence: `docs/evidence/2026-11-20-strict-gc.md`

## Context

ADR-0019 kept strict mode `Unavailable` everywhere because no adapter had been
executed against the full escape/resource matrix, and the first development
host could not create any namespace. A disposable Ubuntu 26.04 VM with a real
kernel, writable cgroup2, passwordless sudo for test setup and the packaged
Bubblewrap AppArmor profile became available. It can create user, mount, PID,
network, UTS and IPC namespaces as an ordinary user through `bwrap`.

## Decision

### 1. Dynamic, probe-gated availability

`PlatformCapabilities::detect()` on Linux runs a real Bubblewrap smoke test
(`--unshare-all --as-pid-1`, read-only runtime bind, private `/proc` and
`/dev`). Strict is `Available { mechanism: "bubblewrap ..." }` only when that
probe succeeds; otherwise it stays `Unavailable` with the real reason. There is
still no fallback to `project`.

### 2. L1 scope: pure language and native Math only

Strict accepts `run`/`call` of documents using the interpreter, console and
native Math. It rejects `--allow-read`, `--allow-write`, `--allow-foreign`,
`--allow-plugin`, `--plugin`, `--trust-native-plugin`, `--kernel` and
`--trace-file` with exit 2 before any sandbox starts. Native ABI, worker
plugins and Foreign kernels are therefore not reachable in strict mode in this
release; L2 inheritance is future work.

### 3. Construction

The trusted supervisor is this process; the restricted child is the same
`marqdo` executable re-executed under Bubblewrap:

* `--unshare-all --as-pid-1 --die-with-parent --new-session` — user, mount,
  PID, network, UTS, IPC and cgroup namespaces; the child is PID 1;
* read-only binds of `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64` and
  `/etc/ld.so.cache`, and of the project tree at `/project`;
* a read-only bind of the program at `/run/marqdo-program`; no host `/root`,
  `$HOME`, `/etc` or other project directories are visible;
* private `/proc`, minimal `/dev`, tmpfs `/tmp`;
* `--clearenv`, `no_new_privs`, `--cap-drop ALL`;
* hard rlimits: CPU, address space (512 MiB), file size, open files (64),
  processes (64), stack (8 MiB), core (0);
* a supervisor deadline that kills the process group, and bounded capture of
  stdout/stderr.

The document argument is rewritten to `/project/...`; only the document token
changes, all other arguments are preserved.

### 4. Child self-verification

The child accepts `MARQDO_STRICT_ACTIVE=1` only if it can verify all of:
`getpid() == 1`, `NoNewPrivs: 1`, empty `CapEff`, a non-initial uid map, and no
non-loopback network interface. A forged marker outside the sandbox fails
before project code runs.

### 5. Threat model and non-goals

This protects against untrusted Marqdo documents that use the language,
interpreter, Math, console, or attempt direct syscalls. It does not sandbox
native plugins (rejected), Foreign kernels (rejected), or a malicious kernel.
Bubblewrap is the construction mechanism, not a certificate; the runtime probe
and self-verification are part of the contract.

Known limits: rlimits are used instead of cgroup-v2 accounting (the VM has
writable cgroups, but the released adapter does not yet claim cgroup
enforcement); filesystem TOCTOU is limited by read-only mounts but not proven
against a racing host user; the project tree is exposed read-only in full;
network is isolated by a network namespace rather than a seccomp allowlist. No
worker/Foreign L2 claim is made.

## Evidence

Executed in a disposable Ubuntu 26.04 VM (kernel 7.0.0, `container: none`,
Bubblewrap 0.11.1, AppArmor `bwrap-userns-restrict` profile, ordinary user
`dev`):

* 7/7 adversarial tests: identity, host reads, writes, `/proc`/privileges,
  network, fd/process limits, memory/CPU/output limits;
* CLI: `run`, `call`, absolute paths, multi-module, default `index.mq.md` and
  `--json` all succeed under strict; Foreign/ABI/plugin/kernel/trace flags are
  rejected with exit 2;
* forged `MARQDO_STRICT_ACTIVE=1` outside the sandbox exits 1;
* removing `/usr/bin/bwrap` makes strict `Unavailable` and the run exits 1 with
  no project execution.

## Consequences

* Linux strict is available **only where the probe succeeds**; this container
  still reports unavailable.
* `worker_processes` remains true for crash isolation, but that is not part of
  the strict claim.
* A future L2 ADR must extend the same supervisor to worker/Foreign processes,
  with its own escape tests, before those may run under strict.
* A remote self-hosted security-lane URL is still required before a release
  calls G-C CI-verified; the local VM evidence is recorded separately.
