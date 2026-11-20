# Linux strict-isolation security gate (G-C) — evidence report

Status: **G-C NOT PASSED. `strict` remains `Unavailable` and fails closed.**
Snapshot: workspace `0.1.0`, 2026-11-20. No release tag.

This report records what was actually probed on the development host and why no
`StrictIsolation::Available` claim is made. It is evidence, not a plan to
weaken the gate.

## 1. Host and capability probe

Command output (abridged; host name removed):

```text
user: uid=0(root) gid=0(root)
kernel: Linux 7.0.0-43-generic x86_64
virt: container-other
apparmor_restrict_unprivileged_userns: 1
max_user_namespaces: 60339
cgroup2 mount: ro,nosuid,nodev,noexec,relatime,nsdelegate,memory_recursiveprot
cgroup controllers: cpuset cpu io memory hugetlb pids rdma misc dmem
unshare --user --map-root-user true
  -> unshare: write failed /proc/self/uid_map: Operation not permitted (exit 1)
unshare --user --map-root-user --net true
  -> unshare: unshare failed: Operation not permitted (exit 1)
bwrap --unshare-user --unshare-net --ro-bind / / true
  -> bwrap: No permissions to create a new namespace (exit 1)
sudo -n true
  -> sudo: The "no new privileges" flag is set, which prevents sudo from
     running as root (exit 1)
```

The workspace root is writable, but the host root filesystem and cgroup2 mount
are read-only. The process is root inside a container with `no_new_privs` set;
`sudo` cannot gain privileges and the parent container refuses namespace
creation. `user.max_user_namespaces` being non-zero does not help because the
container policy/AppArmor boundary rejects the actual `uid_map` write.

## 2. Consequence

- A real Linux adapter (namespaces + seccomp/no_new_privs + cgroup v2 + fd and
  process-tree control) cannot be built, applied and verified here.
- Implementing an adapter and flipping `Available` without executing the escape
  and resource-limit tests would be a false security claim. It was not done.
- `PlatformCapabilities::detect()` continues to report `unavailable` with a
  reason; `enforce` refuses before any host/plugin is constructed. There is no
  fallback to `project`.
- Native plugin loading (`--trust-native-plugin`) is unrelated to strict mode
  and remains an explicit in-process trust decision; the broker and worker
  transport are not OS isolation.

Command evidence:

```bash
marqdo run examples/hello.mq.md --mode strict
# exit 1, stderr: refusing to run in strict mode: ...
marqdo inspect examples/hello.mq.md --platform --json
# platform.strict.state == "unavailable"
```

## 3. What G-C still requires

1. A controllable Ubuntu VM or self-hosted runner that permits user, mount,
   PID and network namespaces and delegated cgroup v2 controllers, reachable
   from an ordinary (non-root) test user.
2. A trusted supervisor that applies filesystem/network/process/resource
   policy and reaps the child tree, plus a runtime probe that reports the
   mechanisms actually applied.
3. Execution of the full negative matrix from
   `docs/STABLE_RELEASE_IMPLEMENTATION.md` §10.5 with a malicious helper:
   direct syscall reads/writes, `/proc` and inherited-fd access, IPv4/IPv6/DNS
   and Unix sockets, fork/exec, CPU/memory/fd/process/output limits, and
   `--trust-native-plugin` rejection.
4. A security-lane CI URL on the same commit, and only then an ADR that flips
   `Unavailable` to `Available` for that specific platform.

Until all four hold, any release must state: **strict OS isolation is not
available; untrusted code must not be executed with Marqdo expecting isolation.**
