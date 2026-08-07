# Step 0 — A clean room (optional)

Everything in this guide works directly on your machine — if you like
living that way, skip to [Step 1](./install.md) and go. This page is for
the rest of us: a disposable [incus](https://linuxcontainers.org/incus/)
system container that behaves like a fresh machine, snapshots before you
start, and restores to factory-new in one command when you want to walk
again — or made a mess.

## Host minimum

The only thing your actual machine needs:

```sh
sudo apt-get update && sudo apt-get install -y incus
sudo adduser "$USER" incus-admin   # manage incus without sudo — log out/in once
sudo incus admin init --minimal    # one-time: default network + storage
```

> If Docker is (or was) installed on this host, its firewall rules break
> container networking — the container gets an address but no internet.
> The suite retired Docker for exactly this reason (portfolio ADR-0014);
> the [Removing Docker appendix](./appendix-de-docker.md) has the
> cleanup script.

## Create the container

First, a project of its own — it keeps the guide's containers (and
their snapshots) apart from anything else your incus hosts, and it
sidesteps hosts whose current project is restricted (restricted
projects block snapshot creation, which this guide leans on):

```sh
# instances-only: images, profiles, networks, and storage stay shared
# with the default project, so launches work with no further setup
incus project create taps \
    -c features.images=false -c features.profiles=false \
    -c features.networks=false \
    -c features.storage.volumes=false -c features.storage.buckets=false
incus project switch taps
```

(`switch` sets your client's active project — everything below just
works, no `--project` flags. Switch back to your usual one afterwards
with `incus project switch default`. Walked before? If the project
still exists, `create` errors — harmless; the `switch` is what
matters.)

Then one block — launch, wait for boot, add a sudo-capable user (the
guide's commands assume one), and bake in what every walk needs — git,
nested incus, mDNS — so a snapshot restore doesn't cost a reinstall:

```sh
# launch: nesting lets later steps run containers inside this one —
# Step 2's KB appliance and Step 6's throwaway forge
incus launch images:ubuntu/24.04 walk -c security.nesting=true

# wait for boot: first an IP (the real readiness signal — network up,
# DHCP done), then let systemd finish settling
until incus list walk -c4 -f csv | grep -q '\.'; do sleep 1; done
incus exec walk -- systemctl is-system-running --wait >/dev/null 2>&1 || true

# a user with passwordless sudo
incus exec walk -- bash -c '
  adduser --disabled-password --gecos "" dev
  echo "dev ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/dev
  chmod 0440 /etc/sudoers.d/dev'

# git and incus now, so a snapshot restore doesn't cost a reinstall
# (incus: Step 2's KB appliance runs as a container inside this one)
incus exec walk -- bash -c '
  apt-get update && apt-get install -y git incus
  incus admin init --minimal
  adduser dev incus-admin
  # nested idmap: the incus package delegates a billion uids to root —
  # more than this container itself has, so nested launches die in
  # newuidmap. Shrink the delegation; nested containers share it.
  sed -i "/^root:/d" /etc/subuid /etc/subgid
  echo "root:1000000:65536" | tee -a /etc/subuid /etc/subgid >/dev/null
  systemctl restart incus'

# mDNS: later steps bring up web apps in here, and this is what lets
# your host browser reach them as http://walk.local:<port>. Advertise
# eth0 only — the nested bridge address is unreachable from outside.
incus exec walk -- bash -c '
  apt-get install -y avahi-daemon
  sed -i "s/^#*allow-interfaces=.*/allow-interfaces=eth0/" /etc/avahi/avahi-daemon.conf
  systemctl restart avahi-daemon'
```

One check back on the **host** before snapshotting:

```sh
getent hosts walk.local   # → the container's eth0 address
```

Empty? Your host isn't resolving mDNS — standard on desktop distros,
absent on some servers: `sudo apt-get install -y avahi-daemon libnss-mdns`
and check again.

## Snapshot before you touch anything

```sh
incus snapshot create walk pristine
```

From here on, a do-over is always one command away:

```sh
incus snapshot restore walk pristine   # factory-fresh again, instantly
```

## Working from a local checkout?

Skip this if you'll clone from GitHub like Step 1 says. But if you're
evaluating local changes (or, like us, dogfooding unpushed work), mount
your checkout in read-only, clone from it, and detach — one block, run
**from your checkout's root** on the host:

```sh
# expose the checkout (the current directory) to the container, read-only
incus config device add walk taps-src disk \
    source="$PWD" path=/mnt/taps-src shift=true readonly=true

# clone it inside (safe.directory: the mount looks foreign-owned to git,
# which refuses to read repos you don't own unless you vouch for them)
incus exec walk -- su - dev -c '
  git config --global --add safe.directory /mnt/taps-src/.git
  git clone file:///mnt/taps-src taps'

# detach — the clean room keeps no window into your machine
incus config device remove walk taps-src
```

The clone's `origin` points at the now-vanished mount — fine for a walk
(nothing pushes from a clean room); use the normal GitHub URL when you
want a live remote. Then skip Step 1's clone — you already have
`~/taps`. Need newer commits later? Repeat the dance: re-add the
device, `git -C ~/taps pull` inside, remove the device.

> A `snapshot restore` reverts *devices* along with the filesystem —
> restoring `pristine` drops the mount (good: no lingering host access)
> and the clone with it, so a re-walk repeats this block.

## Step inside

The last move — everything after this happens in here:

```sh
incus exec walk -- su - dev
```

That shell behaves exactly like a bare machine's: Step 1's
prerequisites block and everything after run unchanged.
