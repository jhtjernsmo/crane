# Checks and runs

Every request can check its answer. Checks are comment lines in the `.http` file, so other tools that read `.http` files ignore them.

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

## Run all

**Run all** in the sidebar sends every request in the open folder, in path order, and shows which passed. Name files `01-login.http`, `02-orders.http` and so on to set the order; values kept with `@save` are available to the requests after them.

Against an environment marked as production, Run all asks first if any request changes data (POST, PUT, PATCH, DELETE), and Rust refuses to run them without that confirmation.

The same run works in a terminal or CI with [crane-cli](cli.md).
