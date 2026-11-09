# Stacks: crane.toml

A `crane.toml` next to your `.http` files lists the services your API needs. **Start stack** in the Containers view (or `crane-cli run --stack`) pulls missing images, starts the services in the order of `depends_on`, waits for each one's ready check, and opens their ports.

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

## Fields

| Field | Means |
| --- | --- |
| `name` | The service's name; others reach it by this name |
| `image` | Any image from Docker Hub, GHCR or another OCI registry, like `postgres:16-alpine` |
| `env` | Environment variables; values can use `{{variables}}` |
| `ports` | `"8080:80"`: port 8080 on this computer to 80 in the stack, on 127.0.0.1 only |
| `volumes` | `"./data:/var/lib/data"`, with `:ro` for read-only; the left side must be inside the folder |
| `command` | Replaces the image's command (not its entrypoint) |
| `user` | Run as this user or uid instead of the image's |
| `depends_on` | Services that must be ready first |
| `ready` | `{ port = 5432 }`, `{ http = "/health", port = 8080 }` or `{ log = "ready to accept" }`, with an optional `timeout` in seconds (default 60) |

If a service stops or is not ready in time, the whole stack is stopped again and its last log lines are shown.

See [containers.md](containers.md) for how the engine runs them.
