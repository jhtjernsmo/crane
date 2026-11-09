# Crane

A desktop app for testing APIs, both public ones and the ones on your own machine or network. Later it will also start and stop the containers those APIs need, so the thing you test and the requests you send live in one place.

Open source under the MIT license. Built with Tauri 2 (Rust) and React.

## Security first

Crane is built for teams that cannot send their API keys to someone else's cloud.

- No account, no cloud, no telemetry. The app only talks to the addresses you send requests to.
- Secrets never go in collection files. Keys and passwords live in the system keyring; files only say `{{token}}`, so collections are safe to keep in git.
- Crane warns before saving anything that looks like a key or token.
- Secrets are masked on screen and never written to history.
- Environments marked as production ask before sending anything that changes data.
- A locked-down Tauri shell (strict CSP, minimal permissions), signed updates and dependency audits in CI.

## What works now (steps 1 to 9)

- Send a request with any method to any address. Leave out `http://` or `https://` and Crane picks: `http://` for `localhost`, `127.*`, `10.*` and `192.168.*`, `https://` for everything else.
- See the status, time, size, the body (JSON is indented and coloured), the headers, and where the time went: name lookup, connect + TLS, waiting for the server and download. Every send opens a fresh connection, so the numbers are not flattered by a reused one.
- Errors in plain words: "Nothing answered at localhost:8080. Is the server running, and on that port?" instead of a stack of library errors.
- Query, Headers and Body tabs. The query table and the address stay in step; edit either one.
- Headers can be switched off without deleting them, and common names are suggested.
- Body as JSON, plain text or a form. Crane adds the matching `Content-Type` unless you set your own. JSON is checked as you type ("Line 3, column 12: A comma is not allowed before the closing bracket.") and can be tidied with one click.
- Values of `Authorization`, `Cookie` and names with token, key, secret or password are hidden on screen until you click show, and are never kept in history. Reopening an old request takes those values from what is on screen now.
- Environments (Local, Test, Prod, ...) with variables you use as `{{base}}` or `{{token}}` in the address, headers and body. A line under the address shows what each one turns into before you send.
- Secret variables live in the system keyring (GNOME Keyring or KWallet on Linux, Keychain on macOS, Credential Manager on Windows). The UI can store a value but never read it back: Rust fills secrets in right before sending, and hides them again (`‹token hidden›`) if the server echoes them in the body, headers or address. Plain values are saved in `environments.json` in the app's data folder, readable only by you.
- Crane warns when a plain variable looks like a key (Stripe, GitHub, Slack, AWS, JWT and other long random strings) and suggests a variable when a token is typed straight into a header.
- Internal APIs: each environment can trust an extra CA (your company's, or a self-signed server certificate), send a client certificate for mutual TLS, and use the system proxy, no proxy or its own (HTTP, HTTPS or SOCKS5, with a `{{secret}}` for the password). Skipping certificate checks is possible for local development only; production environments refuse it.
- An environment marked as production asks before POST, PUT, PATCH and DELETE. The check runs in Rust too, so the UI cannot skip it.
- Collections: open a folder (for example in a git repository) and keep requests there as `.http` files, the same format VS Code REST Client and JetBrains IDEs read. Folders show as a tree; Ctrl+S saves, and Crane asks before it throws away unsaved changes or deletes a file.
- Before a file is written, Rust scans it for keys, tokens and passwords. If it finds one, nothing is written: you can move it to the keyring as a `{{variable}}` with one click, or save it anyway on purpose.
- File commands only reach inside the folder you opened. Paths with `..`, absolute paths and symlinks that lead out are refused, and the folder picker is opened by Rust, so the UI has no file access of its own.
- Containers with Crane's own engine, no Docker or Podman needed (Linux for now). Pull images straight from Docker Hub, GHCR or any OCI registry (checked against their sha256 digests), run them, open their ports on 127.0.0.1, read their logs, stop and remove them. Click a port to use it as a request address.
- Stacks: a `crane.toml` next to your requests lists the services they need. Start stack pulls what is missing, starts the services in the order of `depends_on`, waits for each one's ready check (a port, an HTTP path or a line in the log), opens their ports and shows their logs. If one fails, everything is stopped again and you see its last log lines.
- Checks on answers: each request can have lines like `status 2xx`, `time < 500ms` or `json $.id = {{user_id}}`, kept as `# @expect` comments in its file. `# @save token = json $.access_token` keeps a value for the requests after it. Run all sends every request in the folder and shows what failed; `crane-cli` does the same in a terminal or CI.
- History that is kept between runs (500 sends). Only the request and a summary of the answer (status, time, size) are saved, never the answer itself, and Rust replaces anything that looks like a secret before writing. Clear it any time. Click one to bring the request and its answer back.
- Light, dark or follow the system.

Requests are sent from Rust, not from the webview, so there is no CORS and local addresses work like they do from curl.

## Next steps

1. A policy file for IT, a written threat model (SECURITY.md), dependency audits in CI, and signed releases.

## Checks

```http
# @name Log in
# @expect status 200
# @expect json $.access_token exists
# @save token = json $.access_token
POST {{base}}/login
Content-Type: application/json

{"user": "ada", "password": "{{password}}"}
```

| Check | Means |
| --- | --- |
| `status 200`, `status 2xx` | The status code, or its class |
| `time < 500ms`, `time < 2s` | Total time |
| `header content-type contains json` | Also `exists`, `=`, `!=` |
| `body contains "ok"` | The body as text |
| `json $.items length 3` | Also `exists`, `=`, `!=`, `contains`, `<`, `>`; paths like `$.a.b[0].c` |

Checks can use variables (`json $.owner.id = {{user_id}}`). Values from `@save` count as secrets: they are hidden in answers, errors and reports. A request without checks passes when its status is below 400.

## crane-cli

Runs a folder (or one `.http` file) in path order, so name files `01-login.http`, `02-...` to set the order.

```sh
crane-cli run api-tests --env Test --junit report.xml
```

- `--env NAME` uses an environment from the app. `--envs FILE` reads environments from a file instead; they never hold secret values, so a CI environment can live in the repository.
- Secrets come from the keyring, or in CI from `CRANE_VAR_<name>` environment variables (or `--var name=value`). Both are hidden in the output.
- `--stack` starts the services in `crane.toml` first and stops them afterwards (Linux).
- `--junit FILE` writes a report CI systems can show. `--bail` stops at the first failure. Requests that change data in a production environment need `--yes-production`.
- Exit code 0 when everything passed, 1 when something failed, 2 when the run could not start.

A GitHub Actions job:

```yaml
- run: crane-cli run api-tests --envs ci-env.json --env CI --stack --junit report.xml
  env:
    CRANE_VAR_password: ${{ secrets.API_PASSWORD }}
```

## crane.toml

```toml
[[service]]
name = "db"
image = "postgres:16-alpine"
env = { POSTGRES_PASSWORD = "{{db_password}}" }   # from the active environment; secrets from the keyring
ports = ["5432:5432"]                              # this computer : container, on 127.0.0.1
volumes = ["./data/db:/var/lib/postgresql/data"]   # must stay inside this folder
ready = { port = 5432 }

[[service]]
name = "api"
image = "ghcr.io/your-org/api:latest"
env = { DATABASE_URL = "postgres://postgres:{{db_password}}@db:5432/postgres" }
ports = ["8080:8080"]
depends_on = ["db"]
ready = { http = "/health", port = 8080, timeout = 90 }
```

All services share one network, so `api` reaches the database as `db:5432`. Volumes can only point inside the collection's folder: a `crane.toml` from a repository you cloned cannot hand your other files to a container. Secrets are filled in when the stack starts and live only in the containers' environment, never in the file.

## How the container engine works

`crates/crane-box` is a small container runtime written for Crane:

- **Rootless.** Crane re-runs itself as a helper inside a new user namespace. With an entry in `/etc/subuid` and `/etc/subgid` (most distributions add one per user), the helper maps 65,536 ids through `newuidmap`, so images that switch user, like postgres, work. "root" in a container is your own user outside it.
- **Isolated.** Each container gets its own mount, PID, UTS and IPC namespaces, an overlay on the unpacked image, a fresh `/dev` and its own `/proc`. Paths inside the container are resolved with `openat2(RESOLVE_IN_ROOT)`, so an image's symlinks cannot point a mount at your files. Image layers are unpacked with checks against `..` and absolute paths.
- **No internet by default.** A pod has its own network namespace with only a loopback device. Containers in the same pod reach each other on localhost; Crane forwards the ports you ask for from 127.0.0.1, and nothing else gets in or out.
- **No leftovers.** Each container's pid 1 is a small init that passes on signals and reaps processes, and ends the container when the pod helper goes away, even if it is killed.

Tested live against alpine, traefik/whoami and postgres: `cargo test -p crane-box --test live -- --ignored --test-threads=1`.

## Develop

You need Rust, Node.js and the [Tauri system packages](https://tauri.app/start/prerequisites/) for your OS.

```sh
npm --prefix ui install
npx --prefix ui tauri dev
```

| Task | Command |
| --- | --- |
| Run with hot reload | `npx --prefix ui tauri dev` |
| UI only, in a browser | `npm --prefix ui run dev` (requests use the browser's fetch, so CORS applies) |
| Build installers | `npx --prefix ui tauri build` |
| Rust tests | `cargo test --workspace` |
| Keyring test (uses your real keyring) | `cargo test -p crane -- --ignored` |
| UI tests | `npm --prefix ui test` |
| The CLI | `cargo run -p crane-cli -- run <folder>` |

## Layout

- `crates/crane-core`: the request engine. Sends, measures and explains errors. No Tauri in here, so it is tested on its own against a small local server.
- `crates/crane-core/src/collection.rs`, `http_file.rs` and `secret_scan.rs`: folders of `.http` files, reading and writing them, and the secret check before saving.
- `crates/crane-core/src/env.rs` and `vars.rs`: environments, `{{variables}}` and hiding secrets in responses.
- `crates/crane-box`: the container engine (see above).
- `crates/crane-cli`: `crane-cli run`, for terminals and CI.
- `crates/crane-core/src/checks.rs` and `runner.rs`: the check language and running a folder.
- `src-tauri`: the app shell. Commands to send, list and save environments, and store a secret; `secrets.rs` talks to the system keyring.
- `ui`: the React interface.
