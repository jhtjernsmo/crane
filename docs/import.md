# Importing from Postman, Bruno and Insomnia

Crane reads collections from other API clients and writes them as `.http` files, so moving over does not mean starting again.

| From | What to pick |
| --- | --- |
| Postman | A collection exported as **Collection v2.1** (v2.0 works too), or an exported environment. |
| Bruno | The collection's folder, the one with `bruno.json`. Its `environments` folder comes along. |
| Insomnia | An export in the **v4 (JSON)** format. The newer v5 (YAML) format is not read yet. |

In the app, open the folder you keep requests in, then press **Import** above the tree. From a terminal:

```sh
crane-cli import shop.postman_collection.json --into api-tests
crane-cli import ~/bruno/pets --into api-tests
```

## What happens

- The requests land in a new folder named after the collection, with their folders inside. A second import of the same collection goes into `Name (2)` and never overwrites.
- Files and folders are numbered (`01-log-in.http`, `02-Orders/`), because a run goes in path order. That keeps the order the other tool ran them in.
- Environments are added next to yours. Postman collection variables become an environment named after the collection.
- `{{ _.name }}` in Insomnia becomes `{{name}}`.
- Auth settings become headers: bearer tokens, API keys and basic auth (basic auth only when it has no variables, since the header is encoded ahead of time).

## Secrets

The same rules as saving by hand:

- A token or password typed straight into a request is replaced by a `{{variable}}`, and its value goes into the keyring of the imported environment. The file only ever holds the variable.
- Variables marked secret in Postman or Bruno, and variables with names like `token`, `password` or `api_key`, become secrets. Their values go to the keyring, never to `environments.json`.
- Bruno keeps secret values on the computer, not in its files, so those come over without a value. Crane lists them so you can set them under Environments.

## Scripts

Crane runs no scripts, on purpose. The common test lines become checks instead:

| Postman | Bruno | Crane |
| --- | --- | --- |
| `pm.response.to.have.status(200)` | `res.status: eq 200` | `# @expect status 200` |
| `pm.expect(pm.response.code).to.eql(201)` | | `# @expect status 201` |
| | `res.body.id: isDefined` | `# @expect json $.id exists` |
| | `res.body.name: eq "Rex"` | `# @expect json $.name = "Rex"` |
| `pm.environment.set("token", pm.response.json().token)` | `vars:post-response { token: res.body.token }` | `# @save token = json $.token` |
| `pm.collectionVariables.set("etag", pm.response.headers.get("ETag"))` | | `# @save etag = header etag` |

`var data = pm.response.json()` followed by `pm.environment.set("id", data.id)` is understood too. Everything else in a script is left out, and the import says which requests had lines it could not bring over.

## Not brought over yet

Crane lists each of these after the import, with the requests they were in:

- Pre-request scripts, and test lines other than the ones above.
- File uploads and file bodies. Multipart forms are sent URL-encoded, without their files.
- OAuth 2, AWS, Digest and other auth kinds that need Crane to compute a header.
- Made-up values like `{{$guid}}` and `{{$timestamp}}`, and Insomnia template tags like `{% response %}`.
- gRPC, WebSocket and other requests that are not HTTP.
