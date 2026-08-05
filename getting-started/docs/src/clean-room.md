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
> The suite retired Docker for exactly this reason (portfolio ADR-0014).

## Create the container

One block — launch, wait for boot, add a sudo-capable user (the guide's
commands assume one), and bake in git so a snapshot restore doesn't cost
a reinstall:

```sh
# launch: nesting lets Step 6's throwaway forge run containers inside
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

# git now, so it's in the pristine snapshot
incus exec walk -- bash -c 'apt-get update && apt-get install -y git'
```

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
`~/taps`.

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
