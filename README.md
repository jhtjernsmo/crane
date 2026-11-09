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

## What works now (steps 1 to 6)

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
- History that is kept between runs (500 sends). Only the request and a summary of the answer (status, time, size) are saved, never the answer itself, and Rust replaces anything that looks like a secret before writing. Clear it any time. Click one to bring the request and its answer back.
- Light, dark or follow the system.

Requests are sent from Rust, not from the webview, so there is no CORS and local addresses work like they do from curl.

## Next steps

1. Containers with Crane's own runtime (no Docker or Podman needed): pull images from registries, run them rootless in their own namespaces, forward ports, show logs.
2. A stack per collection in `crane.toml`: start it, wait for health checks, run the requests.
3. Tests on responses and a CLI that runs a collection in CI.

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

## Layout

- `crates/crane-core`: the request engine. Sends, measures and explains errors. No Tauri in here, so it is tested on its own against a small local server.
- `crates/crane-core/src/collection.rs`, `http_file.rs` and `secret_scan.rs`: folders of `.http` files, reading and writing them, and the secret check before saving.
- `crates/crane-core/src/env.rs` and `vars.rs`: environments, `{{variables}}` and hiding secrets in responses.
- `src-tauri`: the app shell. Commands to send, list and save environments, and store a secret; `secrets.rs` talks to the system keyring.
- `ui`: the React interface.
