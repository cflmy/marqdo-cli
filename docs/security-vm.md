# Reproducing the disposable strict-security VM

The G-C lane was executed in a throw-away Ubuntu 26.04 VM because the primary
development host is a container that cannot create namespaces. This document
records how that VM was built so the result can be reproduced and so the VM can
be registered as a dedicated self-hosted security runner.

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

```bash
mkdir -p .tools/vm && cd .tools/vm
curl -L -o resolute.img \
  https://cloud-images.ubuntu.com/resolute/current/resolute-server-cloudimg-amd64.img
qemu-img create -f qcow2 -b resolute.img -F qcow2 disk.qcow2 20G
ssh-keygen -t ed25519 -N '' -f marqdo_vm_key
```

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

From the host repository root (or from `git archive <sha>`):

```bash
tar --exclude=.git --exclude=.tools --exclude=.cache --exclude=.cargo-home \
    --exclude=target --exclude=dist -czf /tmp/marqdo-src.tgz .
scp -P 2222 -i .tools/vm/marqdo_vm_key /tmp/marqdo-src.tgz dev@127.0.0.1:/home/dev/
ssh -p 2222 -i .tools/vm/marqdo_vm_key dev@127.0.0.1
```

Inside the guest:

```bash
mkdir -p ~/marqdo-src && tar -C ~/marqdo-src -xzf ~/marqdo-src.tgz
cd ~/marqdo-src
export PATH="$HOME/.cargo/bin:$PATH"
./tests/security/run-lane.sh
```

Expected final line: `G-C security lane: PASS`. The script exports
`MARQDO_REQUIRE_STRICT_SECURITY=1`; a missing bubblewrap, compiler or helper is
a failure, never a skip.

## 6. Registering as a security runner

Register this VM (not the development container) as a GitHub self-hosted
runner with labels `linux` and `marqdo-security`. The workflow
`.github/workflows/security-linux.yml` targets
`runs-on: [self-hosted, linux, marqdo-security]`. Keep the runner disposable
and recreate it from this document; do not give the runner access to secrets or
the primary workspace. A CI URL from that runner is still required before a
release may call G-C CI-verified.

## Reference environment

```text
Ubuntu 26.04.1 LTS, kernel 7.0.0-43-generic x86_64
QEMU/TCG guest, systemd-detect-virt --container -> none
bubblewrap 0.11.1 (bwrap-userns-restrict)
dev uid 1000, passwordless sudo for setup only
```
