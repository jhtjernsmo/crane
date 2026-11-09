# Contributing to Crane

Thank you for helping. Bug reports, ideas, documentation and code are all welcome.

## Before you start

- **Security problems:** do not open an issue. Report them privately, see [SECURITY.md](SECURITY.md).
- **Bigger changes:** open an issue first, so we can agree on the approach before you spend time on it.
- **Small fixes:** a pull request is fine.

## Setting up

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

The live container tests pull real images and need Linux user namespaces (see [docs/containers.md](docs/containers.md)):

```sh
cargo test -p crane-box --test live -- --ignored --test-threads=1
```

## The website

The site at jhtjernsmo.github.io/crane is plain HTML and CSS in `site/`, published by `.github/workflows/pages.yml`. Open `site/index.html` in a browser to work on it.

## Where things are

- `crates/crane-core`: the request engine. Sends, measures and explains errors. No Tauri in here, so it is tested on its own against a small local server.
- `crates/crane-core/src/collection.rs`, `http_file.rs` and `secret_scan.rs`: folders of `.http` files, reading and writing them, and the secret check before saving.
- `crates/crane-core/src/env.rs` and `vars.rs`: environments, `{{variables}}` and hiding secrets in responses.
- `crates/crane-box`: the container engine (see above).
- `crates/crane-cli`: `crane-cli run`, for terminals and CI.
- `crates/crane-core/src/checks.rs` and `runner.rs`: the check language and running a folder.
- `src-tauri`: the app shell. Commands to send, list and save environments, and store a secret; `secrets.rs` talks to the system keyring.
- `ui`: the React interface.

## How we write code

- **Security first.** Secrets never reach the webview after they are stored, never land in files, history or logs, and every rule that matters is enforced in Rust, not only in the UI. If a change touches secrets, files, the network or containers, say how in the pull request.
- **Tests with every change.** Rust logic gets unit tests next to the code and integration tests in `tests/`; UI behaviour gets a test in `ui/tests/`. CI runs `cargo fmt --check`, `cargo clippy -- -D warnings`, all tests, `cargo audit` and `npm audit`.
- **Plain words in the UI.** Error messages say what happened and what to do ("Nothing answered at localhost:8080. Is the server running?"), not what the library called it.
- **Small, focused commits** with messages like `feat: ...`, `fix: ...`, `docs: ...`, in English, saying what changed and why.

## Pull requests

1. Fork and branch from `main`.
2. Make the change with tests.
3. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `npm --prefix ui test`.
4. Open the pull request and fill in the template.

By contributing you agree that your work is released under the [MIT license](LICENSE).
