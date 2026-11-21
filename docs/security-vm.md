# Reproducing the disposable strict-security VM

The G-C lane was executed in a throw-away Ubuntu 26.04 VM because the primary
development host is a container that cannot create namespaces. This document
records how that VM was built so the result can be reproduced and so the VM can
be registered as a dedicated self-hosted security runner.

> **Current evidence rule.** The lane fixtures are `.mq.md` sources under
> RFC-LANG-0002
> ([`architecture/RFC-LANG-0002-unified-semantics.md`](architecture/RFC-LANG-0002-unified-semantics.md)).
> The lane is only evidence for the **exact SHA in its
> `strict-evidence.json`**; a PASS at an older SHA or on another host is
> historical. Every candidate release must rerun `tests/security/run-lane.sh`
> and archive the evidence document. See [`STATUS.md`](STATUS.md).

**Do not run this on a daily host with valuable data.** The VM is disposable;
the commands below do not disable AppArmor or any global host protection.

## 1. Host prerequisites

- QEMU with TCG (`/dev/kvm` is not required; the reference run had no KVM).
- Network access to `cloud-images.ubuntu.com`, Ubuntu archives and crates.io.
- ~10 GiB free disk and ~3 GiB RAM for the guest.

On an ordinary Ubuntu host, `sudo apt install qemu-system-x86 qemu-utils
genisoimage` is sufficient. In the reference environment QEMU was unpacked
under `.tools/` because the host root was read-only; that directory is
gitignored and is not part of the repository.

## 2. Cloud image and seed

**Pin the base image.** Do not run a release gate from a mutable `current`
URL. Download one dated release, verify it against Ubuntu's published
`SHA256SUMS`, record the digest, and keep that file immutable:

```bash
mkdir -p .tools/vm && cd .tools/vm
# Replace with the reviewed dated release URL, never .../current/...
curl -L -o ubuntu-base.img \
  https://cloud-images.ubuntu.com/releases/26.04/release-20261120/ubuntu-26.04-server-cloudimg-amd64.img
curl -L -o SHA256SUMS \
  https://cloud-images.ubuntu.com/releases/26.04/release-20261120/SHA256SUMS
grep 'ubuntu-26.04-server-cloudimg-amd64.img' SHA256SUMS | sha256sum -c -
sha256sum ubuntu-base.img | tee ubuntu-base.img.sha256
qemu-img create -f qcow2 -b ubuntu-base.img -F qcow2 \
  "strict-${SOURCE_SHA}.qcow2" 24G
ssh-keygen -t ed25519 -N '' -f "marqdo_vm_key-${SOURCE_SHA}"
```

The local reference run used this base image; the digest belongs in the
evidence document as `runner_image_sha256`, not in a claim about a remote
runner:

```text
ubuntu-base.img sha256:
7b90593f5532b49b7e3fe3d2743f0d1f0d822e5b07cebcd8e00c0df8069ecab9
```

Create a fresh qcow2 overlay and a fresh cloud-init seed for every tested SHA.
Never reuse a writable overlay that may contain state from another run.

Create `user-data` with an ordinary `dev` user (passwordless sudo only for
test setup), `meta-data` with a unique instance id, and wrap both into a
NoCloud ISO with volume label `cidata`. Any ISO builder works; the reference
run used the pure-Python `pycdlib` wheel extracted under `.tools/py`:

```bash
python3 - <<'PY'
import pycdlib
iso = pycdlib.PyCdlib()
iso.new(interchange_level=3, vol_ident='cidata', joliet=3, rock_ridge='1.09')
for name, iso_name in {'user-data': 'USER_DATA.;1', 'meta-data': 'META_DATA.;1'}.items():
    iso.add_file(name, f'/{iso_name}', rr_name=name, joliet_path=f'/{name}')
iso.write('seed.iso')
iso.close()
PY
```

`user-data` must contain the public key:

```yaml
#cloud-config
users:
  - name: dev
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    groups: [sudo]
    ssh_authorized_keys:
      - ssh-ed25519 AAAA... your-key
ssh_pwauth: false
package_update: false
package_upgrade: false
```

## 3. Boot and connect

```bash
qemu-system-x86_64 -accel tcg,thread=multi -cpu max -smp 4 -m 3072 \
  -L /usr/share/qemu \
  -drive file=disk.qcow2,if=virtio,format=qcow2 \
  -drive file=seed.iso,if=virtio,format=raw,readonly=on \
  -netdev user,id=n0,hostfwd=tcp:127.0.0.1:2222-:22 \
  -device virtio-net-pci,netdev=n0 \
  -display none -serial file:console.log -monitor none -no-reboot
```

Wait for SSH, then:

```bash
ssh -p 2222 -i marqdo_vm_key dev@127.0.0.1
```

## 4. Guest setup

```bash
sudo apt-get update
sudo apt-get install -y bubblewrap build-essential curl python3
bwrap --version
# install Rust (minimal profile)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH="$HOME/.cargo/bin:$PATH"
```

Check the mechanisms **before** running the lane:

```bash
systemd-detect-virt --container          # must be "none"
unshare --user --map-root-user true      # root may need sudo; ordinary user uses bwrap
bwrap --unshare-user --unshare-net --ro-bind / / true; echo $?
cat /proc/self/uid_map
findmnt -t cgroup2
```

Ubuntu 26.04 keeps `kernel.apparmor_restrict_unprivileged_userns=1`. The
packaged `bwrap-userns-restrict` AppArmor profile is what allows the ordinary
user to run Bubblewrap; the lane does **not** require disabling AppArmor
globally.

## 5. Copy the source and run the lane

Build the archive from the reviewed commit, never from a dirty working tree:

```bash
export SOURCE_SHA="$(git rev-parse HEAD)"        # must be the reviewed SHA
git status --porcelain                            # must be empty
git archive --format=tar "$SOURCE_SHA" > "marqdo-${SOURCE_SHA}.tar"
sha256sum "marqdo-${SOURCE_SHA}.tar" | tee "marqdo-${SOURCE_SHA}.tar.sha256"
scp -P 2222 -i .tools/vm/marqdo_vm_key \
  "marqdo-${SOURCE_SHA}.tar" dev@127.0.0.1:/home/dev/
```

Inside the guest (ordinary user, not root):

```bash
sed -n '1p' ~/marqdo-${SOURCE_SHA}.tar.sha256
sha256sum ~/marqdo-${SOURCE_SHA}.tar
mkdir -p ~/marqdo-${SOURCE_SHA}
tar -C ~/marqdo-${SOURCE_SHA} -xf ~/marqdo-${SOURCE_SHA}.tar
cd ~/marqdo-${SOURCE_SHA}
export PATH="$HOME/.cargo/bin:$PATH"
export MARQDO_TESTED_SHA="$SOURCE_SHA"
export MARQDO_SOURCE_ARCHIVE="$HOME/marqdo-${SOURCE_SHA}.tar"
export MARQDO_SOURCE_ARCHIVE_SHA256="sha256:<digest from the host>"
export MARQDO_STRICT_EVIDENCE="$HOME/evidence-${SOURCE_SHA}/strict-evidence.json"
export MARQDO_STRICT_LANE_LOG="$HOME/evidence-${SOURCE_SHA}/gc-lane.log"
mkdir -p "$(dirname "$MARQDO_STRICT_EVIDENCE")"
./tests/security/run-lane.sh
```

Expected final line: `G-C security lane: PASS`, followed by the evidence path.
The evidence document records the tested SHA, archive digest, `uid`, `uname`,
bwrap/rustc/cargo versions, cgroup2 view, runner kind and outcome. A root-only
PASS is rejected. A missing bubblewrap, compiler, helper or skipped test is a
failure, never a skip.

If the archive digest in the guest does not match the host digest, stop: the
transfer or the extraction is wrong and the run is not evidence.

## 6. Registering as a security runner

Register this VM (not the development container) as a GitHub self-hosted
runner with labels `linux` and `marqdo-security`. The workflow
`.github/workflows/security-linux.yml` targets
`runs-on: [self-hosted, linux, marqdo-security]`.

The runner must be **ephemeral**:

* provision a fresh VM (or a self-hosted runner with a genuinely one-shot
  lifetime) for each job, from a pinned base image;
* no persistent home, host mounts, Docker socket, SSH/Git/cloud credentials or
  release token; only the short-lived runner registration credential;
* job `permissions: contents: read`, `actions/checkout` with
  `persist-credentials: false` (already in the workflow);
* do not run arbitrary fork PRs on it: the workflow only runs on `master`
  pushes or an explicit reviewed `source_sha`;
* destroy the whole VM after the job and keep the uploaded
  `strict-evidence.json` and lane log.

A PASS from a persistent runner with production credentials is not acceptable
evidence, even if the test output is green. If no ephemeral runner is
available, report "local disposable-VM PASS, remote G3 UNVERIFIED" instead of a
green CI claim.

## Reference environment

```text
Ubuntu 26.04.1 LTS, kernel 7.0.0-43-generic x86_64
QEMU/TCG guest, systemd-detect-virt --container -> none
bubblewrap 0.11.1 (bwrap-userns-restrict)
dev uid 1000, passwordless sudo for setup only
```
