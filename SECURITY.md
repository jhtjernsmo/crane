# Security

Crane is built for people who cannot send their API keys to someone else's cloud. This page says what it protects, how, and where its limits are, so you can judge it for your own use.

## Reporting a vulnerability

Please report it privately through GitHub: **Security → Report a vulnerability** on this repository. Do not open a public issue. You will get an answer within a week, and a fix or a plan within 30 days. Reporters are credited in the release notes unless they ask not to be.

Only the latest release gets security fixes.

## What Crane protects

| Asset | Where it lives |
| --- | --- |
| Secrets (API keys, tokens, passwords) | The system keyring: Secret Service on Linux, Keychain on macOS, Credential Manager on Windows |
| Plain variables, environments | `environments.json` in the app's data folder, readable only by you (mode 0600) |
| Requests | `.http` files in a folder you choose, often in git |
| History | `history.jsonl` in the app's data folder (0600): requests and a summary of each answer, never the answer's body |
| Your files and machine | Reached only through the folder you open, and by containers only through volumes inside that folder |

Crane has no account, no cloud and no telemetry. The only network traffic is the requests you send and the image pulls you start.

## Threats and what Crane does about them

### Secrets leaking into files, history or screenshots

- Secret values go from the UI to the keyring through a command that can store but never read them back. Rust fills them in right before a request is sent; the webview never sees them again.
- Before a request is written to a file, Rust scans it for keys, tokens and passwords (known formats such as Stripe, GitHub, Slack, AWS and Google keys, JWTs and private keys, long random strings, and secret-sounding names). Nothing is written while it finds something. "Save with the secret" exists for deliberate cases and can be turned off by policy.
- History is scrubbed with the same scanner before it is written, and keeps no response bodies.
- Secrets a server echoes back are replaced with `‹name hidden›` in the body, headers, address and error messages. So are values saved with `@save` during a run.

**Limits:** masking catches the secret as written and URL-encoded, at least 4 characters long. A server that echoes it base64-encoded or split up will show it. Secrets passed to containers live in their environment, which other processes of your user can read.

### A compromised user interface

The UI is a webview. If an answer contained something that ran as script there, it could misuse Crane's commands.

- Answers are shown as text: React escapes them, and Crane never uses raw HTML.
- The content security policy allows only Crane's own scripts and no remote content of any kind. Inline style attributes are allowed (the UI sets widths with them); inline scripts are not.
- The webview has no file system or shell access of its own. Folder and file pickers are opened by Rust, and file commands only reach inside the folder you opened (no `..`, no absolute paths, no symlinks that lead out).
- Production confirmations, the policy and path checks are enforced in Rust, not only in the UI.

**Limits:** a compromised UI could still send requests, read and write `.http` files in the open folder, and start containers, like you can.

### An untrusted repository

You might clone a repository with `.http` files and a `crane.toml` written by someone else.

- Opening a folder runs nothing. Requests are sent and stacks started only when you ask.
- Volumes in `crane.toml` must stay inside the folder (checked before and after following symlinks), so a stack cannot be given your SSH keys or other files.
- Containers have no internet and can only open ports on 127.0.0.1.
- Requests to a production environment that change data ask first; `crane-cli` needs `--yes-production`.

**Limits:** a `crane.toml` you start can run any image it names. See the next section.

### An untrusted container image

Crane's container engine (`crates/crane-box`) runs images rootless:

- In a user namespace: "root" inside is your own user outside, and other ids map to your subordinate range from `/etc/subuid`.
- In its own mount, PID, UTS, IPC and network namespaces. The network has only a loopback device: no internet, no access to your LAN or to services on your machine.
- On an overlay over the unpacked image, with a fresh `/dev` holding only null, zero, full, random, urandom and tty, and its own `/proc`.
- Layers are unpacked with path checks (no `..`, no absolute paths), and mounts inside the container are resolved with `openat2(RESOLVE_IN_ROOT)`, so the image's symlinks cannot point a mount at your files.
- Image downloads are checked against their sha256 digests.

**Limits, read these:** this is a young engine, not a hardened sandbox. There is no seccomp filter and there are no CPU or memory limits yet, so a hostile image can use the kernel's full system call surface (as allowed inside user namespaces) and can use up resources. Treat it like Docker or Podman in rootless mode, and do not run images you would not run there.

### Network attackers

- TLS certificates are checked against the system's trust store, plus a CA file you add per environment.
- Skipping certificate checks is an explicit, per-environment choice, refused for production environments and blockable by policy.
- Redirects are followed at most 10 times, and each target is checked against the policy's host rules.

### The supply chain

- `Cargo.lock` and `package-lock.json` are committed; CI installs exactly what they list.
- CI runs `cargo audit` and `npm audit` on every change and every week, and Dependabot proposes updates. A known vulnerability fails the build; crates that are only marked unmaintained (today some deep in Tauri's GTK stack and the keyring's D-Bus libraries) are listed in the log and replaced when their parents update.
- Each release lists every dependency in an SBOM (CycloneDX) and is published with GitHub build provenance attestations, so you can check that a download was built from this repository by its CI: `gh attestation verify <file> --repo jhtjernsmo/crane`.

## For organisations: the policy file

An administrator can place a policy that every Crane on the machine follows, in a file only administrators can write:

- Linux: `/etc/crane/policy.toml`
- macOS: `/Library/Application Support/Crane/policy.toml`
- Windows: `%ProgramData%\Crane\policy.toml`

```toml
contact = "it-security@example.com"            # shown when something is refused
allowed_hosts = ["*.example.com", "localhost"]  # only these hosts, also after redirects
blocked_hosts = ["admin.example.com"]
allow_saving_secrets = false                    # no "save with the secret"
allow_insecure_tls = false                      # no skipping certificate checks
containers = false                              # no container engine, no stacks
keep_history = false                            # nothing written to history
```

Every rule is enforced in Rust in the app and in `crane-cli`. A policy file that cannot be read, has a mistake or has an unknown rule puts Crane in its strictest mode (only localhost, no containers, no history), and the app says so in its header.

## What is not done yet

- A seccomp filter and resource limits for containers.
- Signed automatic updates (releases are attested, but the app does not update itself yet).
- Reproducible builds.
- An independent security review. Until there is one, weigh the above yourself, and ask your security team before using Crane against systems that hold other people's data.
