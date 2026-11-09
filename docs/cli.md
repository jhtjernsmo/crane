# crane-cli

`crane-cli` runs a collection from a terminal or a CI job and checks the answers, with the same rules as the app.

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

## Building it

```sh
cargo build -p crane-cli --release
./target/release/crane-cli --help
```

Releases include a `crane-cli` binary for Linux, macOS and Windows.
