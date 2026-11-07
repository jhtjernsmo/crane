# Crane

A desktop app for testing APIs, both public ones and the ones on your own machine or network. Later it will also start and stop the containers those APIs need, so the thing you test and the requests you send live in one place.

Open source under the MIT license. Built with Tauri 2 (Rust) and React.

## What works now (step 1)

- Send a request with any method to any address. Leave out `http://` or `https://` and Crane picks: `http://` for `localhost`, `127.*`, `10.*` and `192.168.*`, `https://` for everything else.
- See the status, time, size, a timing bar (waiting for the server vs. download), the body (JSON is indented and coloured) and the headers.
- Errors in plain words: "Nothing answered at localhost:8080. Is the server running, and on that port?" instead of a stack of library errors.
- History of what you sent this session. Click one to see its answer again.
- Light, dark or follow the system.

Requests are sent from Rust, not from the webview, so there is no CORS and local addresses work like they do from curl.

## Next steps

1. Headers, query and body editors.
2. Collections: folders of `.http` files you can keep in git.
3. Environments and variables (`{{base}}`, `{{token}}`), with secrets kept out of git.
4. Saved history and a finer timing breakdown (DNS, connect, TLS).
5. Internal APIs: self-signed certificates, your own CA, client certificates, proxies.
6. Containers: list, start, stop, logs and ports, through the Docker API (Podman speaks it too).
7. A stack per collection in `crane.toml`: start it, wait for health checks, run the requests.
8. Tests on responses and a CLI that runs a collection in CI.

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
| UI tests | `npm --prefix ui test` |

## Layout

- `crates/crane-core`: the request engine. Sends, measures and explains errors. No Tauri in here, so it is tested on its own against a small local server.
- `src-tauri`: the app shell. One command, `send_request`, that hands the request to `crane-core`.
- `ui`: the React interface.
