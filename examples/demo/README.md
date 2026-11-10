# Try Crane against the demo API

These requests run against Crane's public demo API at `https://crane-api.tjernsmo.no`, a small server made for trying things out. It answers with what you send, gives out a token at `/login` and keeps a little orders store behind it.

Nothing you send is logged or stored, except orders, which are wiped every hour. Still, do not send real keys to it.

## In the app

1. Add an environment called **Demo** with:
   - `base` = `https://crane-api.tjernsmo.no`
   - `password` = `crane`, marked as **secret**. It goes to your keyring, like a real password would.
2. Under **Saved**, open this folder (`examples/demo`).
3. Press **Run all**. The requests run in file order: log in, save the token with `@save`, create an order, read it back and delete it.

Open `03-masked-echo.http` after the run: the server echoes the token back, and Crane shows `‹token hidden›` instead of the value.

## In a terminal

```sh
CRANE_VAR_password=crane crane-cli run examples/demo --envs examples/demo/environments.json --env Demo
```

## Run the demo API yourself

```sh
cargo run -p crane-demo-api          # listens on 127.0.0.1:8080
CRANE_VAR_password=crane crane-cli run examples/demo --envs examples/demo/environments.json --env Local
```

CI runs this collection against a local demo API on every change.

## Endpoints

| Endpoint | Answers with |
| --- | --- |
| `GET /get`, `POST /post`, `ANY /anything` | The request: method, path, query, headers and body |
| `GET /json` | A sample JSON document |
| `ANY /status/{code}` | That status code, 200 to 599 |
| `ANY /delay/{ms}` | An answer after that many milliseconds, at most 10000 |
| `GET /redirect/{n}` | n redirects, at most 10, ending at `/get` |
| `POST /login` | A token, for `{"username": "...", "password": "crane"}` |
| `GET /me` | Whether the token is valid |
| `GET`, `POST /orders` | The latest 50 orders, or a new one from `{"item": "...", "qty": 1-100}` |
| `GET`, `DELETE /orders/{id}` | One order, or deletes it |

Each address can send 120 requests a minute. The source is in [`crates/crane-demo-api`](../../crates/crane-demo-api) and the server setup in [`deploy/demo-api`](../../deploy/demo-api).
