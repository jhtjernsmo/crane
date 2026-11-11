# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Import from Postman (collections and environments), Bruno (collection folders) and Insomnia (v4 exports), in the app and with `crane-cli import`. Simple test scripts become `# @expect` and `# @save` lines, secrets typed into requests move to the keyring, and Crane lists what it could not bring over. See [docs/import.md](docs/import.md).
- Crane updates itself: a few seconds after start it asks GitHub for a newer release and shows "Update to x" in the title bar. Updates are only installed when signed with Crane's update key. `check_updates = false` in the policy file turns the check off.

## [0.1.2] - 2026-11-10

### Added
- Default headers per environment, such as `Accept` and `Content-Type: application/json`. A request's own header wins.
- Right-click in the Saved tree: save the request on screen into a folder, make a folder inside another, open or delete a file.
- Crane draws its own title bar, so the logo is shown once. macOS keeps its traffic lights.
- `crane-demo-api` and `examples/demo`, a small API and a collection to try Crane against.

### Fixed
- The app showed history, "save with the secret" and skipping certificate checks as turned off by a policy when there was none: the policy reached the UI with the wrong field names.
- Right-click no longer shows the web view's own menu, except in text fields, which keep cut, copy and paste.

## [0.1.1] - 2026-11-09

The first published version. (0.1.0 was tagged but never published: its macOS build failed.)

### Requests
- Send any method to any address, with query, headers and JSON, text or form bodies.
- Timing split into name lookup, connect + TLS, waiting and download.
- Plain-language errors instead of library messages.
- History kept between runs, with a summary of each answer and no secrets.

### Secrets and environments
- Environments with `{{variables}}`; secret values in the system keyring, write-only from the UI.
- Secrets echoed by servers are hidden in answers, errors and reports.
- Production environments ask before requests that change data.
- CA files, client certificates (mutual TLS) and proxies per environment.

### Collections and checks
- Requests as `.http` files in a folder you choose.
- A secret scan before anything is written to a file, with a one-click move to the keyring.
- Checks (`# @expect`) and saved values (`# @save`), Run all, and `crane-cli` for CI with JUnit output.

### Containers
- Crane's own rootless container engine for Linux: pulls OCI and Docker images, runs them in their own namespaces with no internet, forwards ports from 127.0.0.1.
- Stacks from `crane.toml`, started in dependency order with ready checks.

### For organisations
- A policy file to limit hosts, forbid saving secrets and skipping certificate checks, and turn off containers and history.
