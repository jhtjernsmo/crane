# The container engine

`crates/crane-box` is a small container runtime written for Crane:

- **Rootless.** Crane re-runs itself as a helper inside a new user namespace. With an entry in `/etc/subuid` and `/etc/subgid` (most distributions add one per user), the helper maps 65,536 ids through `newuidmap`, so images that switch user, like postgres, work. "root" in a container is your own user outside it.
- **Isolated.** Each container gets its own mount, PID, UTS and IPC namespaces, an overlay on the unpacked image, a fresh `/dev` and its own `/proc`. Paths inside the container are resolved with `openat2(RESOLVE_IN_ROOT)`, so an image's symlinks cannot point a mount at your files. Image layers are unpacked with checks against `..` and absolute paths.
- **No internet by default.** A pod has its own network namespace with only a loopback device. Containers in the same pod reach each other on localhost; Crane forwards the ports you ask for from 127.0.0.1, and nothing else gets in or out.
- **No leftovers.** Each container's pid 1 is a small init that passes on signals and reaps processes, and ends the container when the pod helper goes away, even if it is killed.

Tested live against alpine, traefik/whoami and postgres: `cargo test -p crane-box --test live -- --ignored --test-threads=1`.

## What it needs

- Linux with unprivileged user namespaces (the default on most distributions; Ubuntu 24.04 and later restrict them through AppArmor, see below).
- For images that switch user (postgres, nginx and most databases): an entry for your user in `/etc/subuid` and `/etc/subgid`, and the `newuidmap` and `newgidmap` tools (package `uidmap` on Debian and Ubuntu, `shadow` on Arch and Fedora). Most distributions add the entry when the user is created; check with `grep $USER /etc/subuid`.
- Overlayfs in user namespaces (Linux 5.11 or later).

On Ubuntu 24.04 and later, allow unprivileged user namespaces with `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0`, or add an AppArmor profile for Crane.

macOS and Windows do not have Linux namespaces, so containers need a small Linux virtual machine there. That is planned; until then the Containers view says it is Linux-only.

## Where its files are

Everything lives in the app's data folder under `containers/`: downloaded layers (`blobs/`), unpacked images (`rootfs/`), each container's own changes (`pods/`) and logs (`logs/`). Removing an image in the app frees what no other image uses.

## Limits

This is a young engine, not a hardened sandbox: there is no seccomp filter and no CPU or memory limit yet. Run images you would run with rootless Docker or Podman. See [SECURITY.md](../SECURITY.md).
