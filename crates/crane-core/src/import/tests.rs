use super::*;
use crate::env::MemorySecrets;
use crate::http_file::SavedHeader;

fn header(name: &str, value: &str) -> SavedHeader {
    SavedHeader { name: name.into(), value: value.into(), enabled: true }
}

const POSTMAN: &str = r#"{
  "info": { "name": "Shop API", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}", "type": "string" }] },
  "variable": [{ "key": "base", "value": "https://shop.example.com" }],
  "item": [
    {
      "name": "Log in",
      "request": {
        "auth": { "type": "noauth" },
        "method": "POST",
        "header": [{ "key": "Accept", "value": "application/json" }, { "key": "X-Debug", "value": "1", "disabled": true }],
        "body": { "mode": "raw", "raw": "{\"user\": \"ada\"}", "options": { "raw": { "language": "json" } } },
        "url": { "raw": "{{base}}/login", "host": ["{{base}}"], "path": ["login"] }
      },
      "event": [{ "listen": "test", "script": { "exec": [
        "pm.test(\"ok\", function () {",
        "    pm.response.to.have.status(200);",
        "});",
        "var jsonData = pm.response.json();",
        "pm.environment.set(\"token\", jsonData.access_token);",
        "console.log('done');"
      ] } }]
    },
    {
      "name": "Orders",
      "item": [
        {
          "name": "Get order",
          "request": { "method": "GET", "url": { "raw": "{{base}}/orders/:id", "variable": [{ "key": "id", "value": "" }] } }
        },
        {
          "name": "Search",
          "request": {
            "method": "POST",
            "header": [{ "key": "Authorization", "value": "Bearer abc123def456" }],
            "body": { "mode": "urlencoded", "urlencoded": [{ "key": "q", "value": "red shoes" }, { "key": "skip", "value": "x", "disabled": true }] },
            "url": "{{base}}/search?id={{$guid}}"
          }
        }
      ]
    }
  ]
}"#;

#[test]
fn reads_a_postman_collection() {
    let got = from_text(POSTMAN).unwrap();
    assert_eq!((got.name.as_str(), got.from), ("Shop API", "Postman"));
    assert_eq!(got.requests.len(), 3);

    let login = &got.requests[0].request;
    assert!(got.requests[0].folder.is_empty());
    assert_eq!((login.method.as_str(), login.url.as_str(), login.body_mode.as_str()), ("POST", "{{base}}/login", "json"));
    assert_eq!(login.headers, [header("Accept", "application/json"), SavedHeader { enabled: false, ..header("X-Debug", "1") }]);
    assert_eq!(login.expect, ["status 200"]);
    assert_eq!(login.save, ["token = json $.access_token"]);

    let order = &got.requests[1];
    assert_eq!(order.folder, ["Orders"]);
    assert_eq!(order.request.url, "{{base}}/orders/{{id}}");
    assert_eq!(order.request.headers, [header("Authorization", "Bearer {{token}}")], "collection auth is inherited");

    let search = &got.requests[2].request;
    assert_eq!((search.body_mode.as_str(), search.body.as_str()), ("form", "q=red%20shoes"));
    assert_eq!(search.headers, [header("Authorization", "Bearer abc123def456")], "the request's own header wins over inherited auth");

    assert_eq!(got.environments.len(), 1);
    assert_eq!(got.environments[0].vars, [ImportedVar { name: "base".into(), value: "https://shop.example.com".into(), secret: false }]);
    let notes = got.notes.lines().join("\n");
    assert!(notes.contains("Some test script lines were left out") && notes.contains("(Log in)"), "{notes}");
    assert!(notes.contains("{{$...}}") && notes.contains("(Search)"), "{notes}");
}

#[test]
fn reads_a_postman_environment_with_secrets() {
    let env = r#"{ "name": "Prod", "_postman_variable_scope": "environment", "values": [
        { "key": "base", "value": "https://api.example.com", "enabled": true },
        { "key": "token", "value": "t0ken", "type": "secret", "enabled": true },
        { "key": "old", "value": "x", "enabled": false } ] }"#;
    let got = from_text(env).unwrap();
    assert!(got.requests.is_empty());
    let vars = &got.environments[0].vars;
    assert_eq!(vars.len(), 2);
    assert!(vars[1].secret && vars[1].value == "t0ken");
}

#[test]
fn reads_postman_auth_kinds() {
    let col = r#"{ "info": { "name": "A", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" }, "item": [
        { "name": "basic", "request": { "method": "GET", "url": "x.dev", "auth": { "type": "basic", "basic": [
            { "key": "username", "value": "ada" }, { "key": "password", "value": "pw" } ] } } },
        { "name": "key in query", "request": { "method": "GET", "url": "x.dev/a", "auth": { "type": "apikey", "apikey": [
            { "key": "key", "value": "api_key" }, { "key": "value", "value": "{{key}}" }, { "key": "in", "value": "query" } ] } } },
        { "name": "oauth", "request": { "method": "GET", "url": "x.dev", "auth": { "type": "oauth2" } } } ] }"#;
    let got = from_text(col).unwrap();
    assert_eq!(got.requests[0].request.headers, [header("Authorization", "Basic YWRhOnB3")]);
    assert_eq!(got.requests[1].request.url, "x.dev/a?api_key={{key}}");
    assert!(got.notes.lines().iter().any(|n| n.starts_with("\"oauth2\" auth was left out")));
}

#[test]
fn reads_an_insomnia_export() {
    let export = r#"{ "_type": "export", "__export_format": 4, "resources": [
        { "_id": "wrk_1", "_type": "workspace", "name": "Billing" },
        { "_id": "fld_1", "_type": "request_group", "parentId": "wrk_1", "name": "Invoices", "metaSortKey": 2 },
        { "_id": "req_2", "_type": "request", "parentId": "fld_1", "name": "List", "method": "GET",
          "url": "{{ _.base }}/invoices", "parameters": [{ "name": "page", "value": "2" }], "headers": [],
          "authentication": { "type": "bearer", "token": "{{ _.token }}" }, "metaSortKey": 1 },
        { "_id": "req_1", "_type": "request", "parentId": "wrk_1", "name": "Health", "method": "GET",
          "url": "{{base}}/health", "body": {}, "metaSortKey": 1 },
        { "_id": "env_base", "_type": "environment", "parentId": "wrk_1", "name": "Base", "data": { "base": "http://localhost:3000", "api": { "v": 2 } } },
        { "_id": "env_prod", "_type": "environment", "parentId": "env_base", "name": "Prod", "data": { "base": "https://billing.example.com", "token": "" } }
    ] }"#;
    let got = from_text(export).unwrap();
    assert_eq!(got.name, "Billing");
    let names: Vec<&str> = got.requests.iter().map(|r| r.request.name.as_str()).collect();
    assert_eq!(names, ["Health", "List"], "sorted like Insomnia shows them");
    let list = &got.requests[1];
    assert_eq!(list.folder, ["Invoices"]);
    assert_eq!(list.request.url, "{{base}}/invoices?page=2");
    assert_eq!(list.request.headers, [header("Authorization", "Bearer {{token}}")]);

    assert_eq!(got.environments.len(), 1);
    let prod = &got.environments[0];
    assert_eq!(prod.name, "Prod");
    let base = prod.vars.iter().find(|v| v.name == "base").unwrap();
    assert_eq!(base.value, "https://billing.example.com");
    assert!(prod.vars.iter().any(|v| v.name == "api.v" && v.value == "2"));
}

fn bruno_collection() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::write(p.join("bruno.json"), r#"{ "version": "1", "name": "Pets", "type": "collection" }"#).unwrap();
    std::fs::write(p.join("collection.bru"), "headers {\n  X-Client: crane-test\n}\n\nauth {\n  mode: bearer\n}\n\nauth:bearer {\n  token: {{token}}\n}\n")
        .unwrap();
    std::fs::write(
        p.join("Create pet.bru"),
        "meta {\n  name: Create pet\n  type: http\n  seq: 2\n}\n\npost {\n  url: {{base}}/pets\n  body: json\n  auth: inherit\n}\n\nheaders {\n  Accept: application/json\n  ~X-Debug: 1\n}\n\nbody:json {\n  {\n    \"name\": \"Rex\"\n  }\n}\n\nassert {\n  res.status: eq 201\n  res.body.id: isDefined\n  res.body.name: eq \"Rex\"\n}\n\nvars:post-response {\n  pet_id: res.body.id\n}\n\ntests {\n  test(\"x\", () => {});\n}\n",
    )
    .unwrap();
    std::fs::write(p.join("Log in.bru"), "meta {\n  name: Log in\n  seq: 1\n}\n\npost {\n  url: {{base}}/login\n  body: formUrlEncoded\n  auth: none\n}\n\nbody:form-urlencoded {\n  user: ada\n  pass: {{password}}\n}\n").unwrap();
    std::fs::create_dir_all(p.join("admin")).unwrap();
    std::fs::write(p.join("admin/folder.bru"), "meta {\n  name: Admin tools\n}\n").unwrap();
    std::fs::write(
        p.join("admin/Delete pet.bru"),
        "meta {\n  name: Delete pet\n  seq: 1\n}\n\ndelete {\n  url: {{base}}/pets/:id\n  auth: inherit\n}\n\nparams:path {\n  id: 7\n}\n",
    )
    .unwrap();
    std::fs::create_dir_all(p.join("environments")).unwrap();
    std::fs::write(p.join("environments/Local.bru"), "vars {\n  base: http://localhost:8080\n}\nvars:secret [\n  token,\n  password\n]\n").unwrap();
    dir
}

#[test]
fn reads_a_bruno_collection() {
    let dir = bruno_collection();
    let got = read(dir.path()).unwrap();
    assert_eq!((got.name.as_str(), got.from), ("Pets", "Bruno"));
    let names: Vec<&str> = got.requests.iter().map(|r| r.request.name.as_str()).collect();
    assert_eq!(names, ["Log in", "Create pet", "Delete pet"], "by seq, then folders");

    let login = &got.requests[0].request;
    assert_eq!((login.body_mode.as_str(), login.body.as_str()), ("form", "user=ada&pass={{password}}"));
    assert_eq!(login.headers, [header("X-Client", "crane-test")], "auth none, but collection headers still apply");

    let create = &got.requests[1].request;
    assert_eq!(create.body, "{\n  \"name\": \"Rex\"\n}");
    assert_eq!(create.expect, ["status 201", "json $.id exists", "json $.name = \"Rex\""]);
    assert_eq!(create.save, ["pet_id = json $.id"]);
    assert!(create.headers.contains(&header("Authorization", "Bearer {{token}}")));
    assert!(create.headers.contains(&SavedHeader { enabled: false, ..header("X-Debug", "1") }));

    let delete = &got.requests[2];
    assert_eq!(delete.folder, ["Admin tools"]);
    assert_eq!(delete.request.url, "{{base}}/pets/7");

    let local = &got.environments[0];
    assert_eq!(local.name, "Local");
    assert_eq!(local.vars.iter().filter(|v| v.secret && v.value.is_empty()).count(), 2);
    assert!(got.notes.lines().iter().any(|n| n.starts_with("Scripts and tests were left out") && n.contains("Create pet")));
}

#[test]
fn applying_writes_numbered_files_and_moves_secrets_to_the_keyring() {
    let target = tempfile::tempdir().unwrap();
    let col = Collection::open(target.path()).unwrap();
    let store = MemorySecrets::default();

    let imported = from_text(POSTMAN).unwrap();
    let (envs, report) = apply(&imported, &col, &EnvFile::default(), &store).unwrap();

    assert_eq!(report.folder.as_deref(), Some("Shop API"));
    assert_eq!(report.requests, 3);
    assert_eq!(report.secrets_moved, ["token_2"], "{{token}} is already a variable, so the typed one gets its own name");

    let paths = crate::runner::request_paths(&col.tree().unwrap());
    assert_eq!(paths, ["Shop API/01-log-in.http", "Shop API/02-Orders/01-get-order.http", "Shop API/02-Orders/02-search.http"]);

    let search = col.read("Shop API/02-Orders/02-search.http").unwrap().request;
    assert_eq!(search.headers, [header("Authorization", "Bearer {{token_2}}")]);
    let on_disk = std::fs::read_to_string(target.path().join("Shop API/02-Orders/02-search.http")).unwrap();
    assert!(!on_disk.contains("abc123def456"), "{on_disk}");

    let env = &envs.environments[0];
    assert_eq!(env.name, "Shop API");
    assert_eq!(envs.active.as_deref(), Some(env.id.as_str()));
    let token = env.vars.iter().find(|v| v.name == "token_2").unwrap();
    assert!(token.secret && token.value.is_empty());
    assert_eq!(store.get(&env.id, "token_2").unwrap().as_deref(), Some("abc123def456"));
}

#[test]
fn applying_twice_does_not_overwrite() {
    let target = tempfile::tempdir().unwrap();
    let col = Collection::open(target.path()).unwrap();
    let store = MemorySecrets::default();
    let imported = from_text(POSTMAN).unwrap();
    let (first, _) = apply(&imported, &col, &EnvFile::default(), &store).unwrap();
    let (second, report) = apply(&imported, &col, &first, &store).unwrap();
    assert_eq!(report.folder.as_deref(), Some("Shop API (2)"));
    let names: Vec<&str> = second.environments.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Shop API", "Shop API (2)"]);
}

#[test]
fn secret_values_from_environments_go_to_the_keyring_only() {
    let target = tempfile::tempdir().unwrap();
    let col = Collection::open(target.path()).unwrap();
    let store = MemorySecrets::default();
    let env = r#"{ "name": "Prod", "_postman_variable_scope": "environment", "values": [
        { "key": "api_key", "value": "k-123", "enabled": true },
        { "key": "password", "value": "", "type": "secret", "enabled": true } ] }"#;
    let (envs, report) = apply(&from_text(env).unwrap(), &col, &EnvFile::default(), &store).unwrap();
    assert_eq!(report.folder, None);
    let prod = &envs.environments[0];
    assert!(prod.vars.iter().all(|v| v.secret && v.value.is_empty()), "api_key sounds secret, so it is one");
    assert_eq!(store.get(&prod.id, "api_key").unwrap().as_deref(), Some("k-123"));
    assert_eq!(report.secrets_to_set, ["{{password}} in Prod"]);
}

#[test]
fn explains_what_it_cannot_read() {
    assert!(from_text("type: collection.insomnia.rest/5.0\nname: x").unwrap_err().0.contains("v4 (JSON)"));
    assert!(from_text("not json").unwrap_err().0.contains("not an export"));
    assert!(from_text(r#"{"hello": 1}"#).unwrap_err().0.contains("not a Postman collection"));
    let empty = tempfile::tempdir().unwrap();
    assert!(read(empty.path()).unwrap_err().0.contains("bruno.json"));
}
