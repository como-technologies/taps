# Appendix — Removing Docker

Docker sets the kernel's iptables `FORWARD` policy to DROP and keeps it
that way — which silently breaks routed networking for every other
bridge on the machine, incus's included. The symptom is always the same:
your container gets an IP and local DNS, and nothing outside answers.
The suite retired Docker for exactly this (portfolio ADR-0014), and
this script has now cleaned more than one of our machines.

Save it, then `sudo ./de-docker.sh`:

```sh
#!/usr/bin/env bash
# De-docker a machine: stop Docker, purge its packages, and reset the
# iptables FORWARD policy it leaves behind. Run with sudo.
set -euo pipefail

for u in docker.service docker.socket containerd.service; do
  systemctl list-unit-files "$u" --no-legend 2>/dev/null | grep -q . \
    && systemctl disable --now "$u" || echo "skip: $u (not present)"
done

# purge whatever docker/containerd packages are actually installed
# (docker.io, docker-ce, docker-compose-*, containerd, containerd.io …)
pkgs=$(dpkg -l | awk '/^ii/ {print $2}' | grep -E '^(docker|containerd)' || true)
[ -n "$pkgs" ] && apt-get purge -y $pkgs
apt-get autoremove -y

iptables -P FORWARD ACCEPT
iptables -F FORWARD
echo DE-DOCKERED
```

> ⚠️ **Data warning.** Ubuntu's `docker.io` package **deletes
> `/var/lib/docker` on purge** — images, containers, and *volumes* —
> after a 10-second Ctrl+C window in its maintainer script. (Docker's
> own `docker-ce` packages leave the data directory alone.) If anything
> on the machine ever mattered in Docker, check before running:
>
> ```sh
> docker ps -a && docker volume ls
> ```

No reboot needed: resetting the `FORWARD` policy takes effect
immediately, and incus manages its own firewall rules independently.
Verify from any container:

```sh
incus exec walk -- ping -c1 1.1.1.1
```
