# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - unreleased

The first version.

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
