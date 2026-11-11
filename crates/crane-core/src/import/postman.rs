//! Postman collections (v2.0 and v2.1) and environment exports.
//!
//! Crane runs no scripts, so test scripts are read for the common lines
//! (checking the status, keeping a value from the answer) and those become
//! `# @expect` and `# @save` lines. Everything else is listed in the notes.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::text::{self, Basic};
use super::{ImportError, Imported, ImportedEnv, ImportedRequest, ImportedVar, Notes};
use crate::http_file::SavedRequest;

const SCHEMA_PREFIX: &str = "https://schema.getpostman.com/json/collection/v2";
/// Postman keeps folders inside folders without limit; Crane stops here.
const MAX_DEPTH: usize = 16;

pub fn is_collection(json: &Value) -> bool {
    json["info"]["schema"].as_str().is_some_and(|s| s.starts_with(SCHEMA_PREFIX)) || (json["info"]["name"].is_string() && json["item"].is_array())
}

pub fn is_environment(json: &Value) -> bool {
    json["_postman_variable_scope"].as_str().is_some_and(|s| s == "environment" || s == "globals")
        || (json["name"].is_string() && json["values"].is_array() && json.get("info").is_none())
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_default()
}

fn on(v: &Value) -> bool {
    !v["disabled"].as_bool().unwrap_or(false)
}

/// `[{key, value, disabled}]`, the shape Postman uses for most lists.
fn pairs(list: &Value) -> Vec<(String, String)> {
    list.as_array().into_iter().flatten().filter(|p| on(p) && !s(p, "key").is_empty()).map(|p| (s(p, "key").to_string(), value_text(&p["value"]))).collect()
}

/// Values are usually strings, but numbers and booleans happen.
fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn read_environment(json: &Value) -> Imported {
    let name = s(json, "name");
    let vars = json["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["enabled"].as_bool().unwrap_or(true))
        .map(|v| ImportedVar { name: s(v, "key").to_string(), value: value_text(&v["value"]), secret: s(v, "type") == "secret" })
        .collect();
    Imported {
        name: name.to_string(),
        from: "Postman",
        requests: vec![],
        environments: vec![ImportedEnv { name: if name.is_empty() { "Postman".into() } else { name.into() }, vars }],
        notes: Notes::default(),
    }
}

pub fn read_collection(json: &Value) -> Result<Imported, ImportError> {
    let name = s(&json["info"], "name").to_string();
    let mut out = Imported { name: if name.is_empty() { "Postman".into() } else { name.clone() }, from: "Postman", ..Default::default() };
    walk(&json["item"], &[], &json["auth"], 0, &mut out);

    let vars: Vec<ImportedVar> = json["variable"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| on(v) && !s(v, "key").is_empty())
        .map(|v| ImportedVar { name: s(v, "key").to_string(), value: value_text(&v["value"]), secret: s(v, "type") == "secret" })
        .collect();
    if !vars.is_empty() {
        out.environments.push(ImportedEnv { name: out.name.clone(), vars });
    }
    if json["event"].as_array().is_some_and(|e| !e.is_empty()) {
        out.notes.add("Collection scripts were left out; Crane runs no scripts", "");
    }
    if out.requests.is_empty() && out.environments.is_empty() {
        return Err(ImportError("This Postman collection has no requests.".into()));
    }
    Ok(out)
}

fn walk(items: &Value, folder: &[String], auth: &Value, depth: usize, out: &mut Imported) {
    if depth > MAX_DEPTH {
        out.notes.add("Folders nested deeper than 16 levels were left out", "");
        return;
    }
    for item in items.as_array().into_iter().flatten() {
        let name = s(item, "name").to_string();
        // A folder's own auth is used by everything inside unless they set their own.
        let auth = if item["auth"].is_object() { &item["auth"] } else { auth };
        if item["item"].is_array() {
            let mut inner = folder.to_vec();
            inner.push(if name.is_empty() { "Folder".into() } else { name });
            if item["event"].as_array().is_some_and(|e| !e.is_empty()) {
                out.notes.add("Folder scripts were left out; Crane runs no scripts", "");
            }
            walk(&item["item"], &inner, auth, depth + 1, out);
        } else if !item["request"].is_null() {
            let request = read_request(item, auth, &mut out.notes);
            out.requests.push(ImportedRequest { folder: folder.to_vec(), request });
        }
    }
}

fn read_request(item: &Value, inherited_auth: &Value, notes: &mut Notes) -> SavedRequest {
    let req = &item["request"];
    let name = s(item, "name");
    let name = if name.is_empty() { "Request" } else { name };
    // v2 also allows the request to be just its address.
    let (method, url) = match req {
        Value::String(u) => ("GET".to_string(), u.clone()),
        _ => (s(req, "method").to_ascii_uppercase(), read_url(&req["url"])),
    };
    let mut r = SavedRequest {
        name: name.to_string(),
        method: if method.is_empty() { "GET".into() } else { method },
        url,
        headers: req["header"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|h| !s(h, "key").is_empty())
            .map(|h| text::header(s(h, "key"), &value_text(&h["value"]), on(h)))
            .collect(),
        body_mode: "none".into(),
        body: String::new(),
        expect: vec![],
        save: vec![],
    };
    read_body(&req["body"], &mut r, notes);
    let auth = if req["auth"].is_object() { &req["auth"] } else { inherited_auth };
    apply_auth(auth, &mut r, notes);
    for event in item["event"].as_array().into_iter().flatten() {
        let script = event["script"]["exec"].as_array().map(|lines| lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        match s(event, "listen") {
            "test" => read_tests(&script, &mut r, notes),
            _ if script.trim().is_empty() => {}
            _ => notes.add("Pre-request scripts were left out; Crane runs no scripts", name),
        }
    }
    if text::uses_made_up_values(&r) {
        notes.add(text::MADE_UP_VALUES, name);
    }
    r
}

fn read_url(url: &Value) -> String {
    let raw = match url {
        Value::String(u) => return u.clone(),
        _ => s(url, "raw").to_string(),
    };
    let built = if raw.is_empty() { build_url(url) } else { raw };
    let params: Vec<(String, String)> = url["variable"].as_array().into_iter().flatten().map(|v| (s(v, "key").to_string(), value_text(&v["value"]))).collect();
    text::fill_path_params(&built, &params)
}

/// For exports that only have the pieces of the address.
fn build_url(url: &Value) -> String {
    let join = |v: &Value, sep: &str| match v {
        Value::Array(parts) => parts.iter().map(value_text).collect::<Vec<_>>().join(sep),
        other => value_text(other),
    };
    let protocol = s(url, "protocol");
    let host = join(&url["host"], ".");
    let path = join(&url["path"], "/");
    let base = match (protocol.is_empty(), path.is_empty()) {
        (true, true) => host,
        (true, false) => format!("{host}/{path}"),
        (false, true) => format!("{protocol}://{host}"),
        (false, false) => format!("{protocol}://{host}/{path}"),
    };
    text::with_query(&base, &pairs(&url["query"]))
}

fn read_body(body: &Value, r: &mut SavedRequest, notes: &mut Notes) {
    if body.is_null() || body["disabled"].as_bool().unwrap_or(false) {
        return;
    }
    match s(body, "mode") {
        "raw" => {
            r.body = s(body, "raw").to_string();
            let language = s(&body["options"]["raw"], "language");
            r.body_mode = if language == "json" { "json" } else { "text" }.into();
            let content_type = match language {
                "xml" => Some("application/xml"),
                "html" => Some("text/html"),
                "javascript" => Some("application/javascript"),
                _ => None,
            };
            if let Some(ct) = content_type {
                text::add_header(r, "Content-Type", ct);
            }
        }
        "urlencoded" => {
            r.body_mode = "form".into();
            r.body = text::form_body(&pairs(&body["urlencoded"]));
        }
        "formdata" => {
            let fields = body["formdata"].as_array().cloned().unwrap_or_default();
            if fields.iter().any(|f| s(f, "type") == "file") {
                notes.add("File uploads were left out; Crane cannot send files yet", &r.name);
            }
            let text_fields: Vec<(String, String)> = fields
                .iter()
                .filter(|f| on(f) && s(f, "type") != "file" && !s(f, "key").is_empty())
                .map(|f| (s(f, "key").to_string(), value_text(&f["value"])))
                .collect();
            r.body_mode = "form".into();
            r.body = text::form_body(&text_fields);
            notes.add("Multipart forms are sent URL-encoded; Crane has no multipart yet", &r.name);
        }
        "graphql" => {
            let query = s(&body["graphql"], "query");
            let vars_text = s(&body["graphql"], "variables");
            let vars: Value = serde_json::from_str(vars_text).unwrap_or(Value::Null);
            let doc = serde_json::json!({ "query": query, "variables": vars });
            r.body_mode = "json".into();
            r.body = serde_json::to_string_pretty(&doc).unwrap_or_default();
        }
        "file" => notes.add("File bodies were left out; Crane cannot send files yet", &r.name),
        _ => {}
    }
}

/// Postman v2.1 keeps auth settings as `[{key, value}]`, v2.0 as an object.
fn auth_value(auth: &Value, kind: &str, key: &str) -> String {
    match &auth[kind] {
        Value::Array(list) => list.iter().find(|p| s(p, "key") == key).map(|p| value_text(&p["value"])).unwrap_or_default(),
        obj => value_text(&obj[key]),
    }
}

fn apply_auth(auth: &Value, r: &mut SavedRequest, notes: &mut Notes) {
    match s(auth, "type") {
        "" | "noauth" | "inherit" => {}
        "bearer" => text::add_header(r, "Authorization", &format!("Bearer {}", auth_value(auth, "bearer", "token"))),
        "basic" => match text::basic_auth(&auth_value(auth, "basic", "username"), &auth_value(auth, "basic", "password")) {
            Basic::Header(h) => text::add_header(r, "Authorization", &h),
            Basic::HasVariables => notes.add(text::BASIC_WITH_VARS, &r.name),
        },
        "apikey" => {
            let key = auth_value(auth, "apikey", "key");
            let value = auth_value(auth, "apikey", "value");
            if auth_value(auth, "apikey", "in") == "query" {
                r.url = text::with_query(&r.url, &[(key, value)]);
            } else if !key.is_empty() {
                text::add_header(r, &key, &value);
            }
        }
        other => notes.add(&format!("\"{other}\" auth was left out; set the header it makes by hand"), &r.name),
    }
}

static STATUS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"pm\.response\.to\.have\.status\(\s*(\d{3})\s*\)",
        r"pm\.expect\(\s*pm\.response\.(?:code|status)\s*\)\.to\.(?:eql|equal|be|eq)\(\s*(\d{3})\s*\)",
        r"pm\.response\.to\.be\.(ok)\b",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid pattern"))
    .collect()
});
static SET_VAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"pm\.(?:environment|collectionVariables|globals|variables)\.set\(\s*["']([A-Za-z_][\w.-]*)["']\s*,\s*(.+?)\s*\)\s*;?\s*$"#)
        .expect("valid pattern")
});
static JSON_VAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:pm\.response\.json\(\)|JSON\.parse\(\s*responseBody\s*\))\s*;?").expect("valid pattern")
});
static HEADER_GET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^pm\.response\.headers\.get\(\s*["']([^"']+)["']\s*\)$"#).expect("valid pattern"));
/// Lines that only hold the script together.
static FRAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(pm\.test\(.*(function\s*\(\s*\)|\(\s*\)\s*=>)\s*\{|\}\s*\)?\s*;?|//.*|)$").expect("valid pattern"));

/// Turns the usual test lines into checks and saves; notes the rest.
fn read_tests(script: &str, r: &mut SavedRequest, notes: &mut Notes) {
    let json_names: Vec<String> = JSON_VAR.captures_iter(script).map(|c| c[1].to_string()).collect();
    let mut left_out = false;
    for line in script.lines().map(str::trim) {
        if let Some(code) = STATUS.iter().find_map(|re| re.captures(line).map(|c| c[1].to_string())) {
            let check = format!("status {}", if code == "ok" { "200" } else { &code });
            if !r.expect.contains(&check) {
                r.expect.push(check);
            }
        } else if let Some(c) = SET_VAR.captures(line) {
            match save_source(&c[2], &json_names) {
                Some(source) => r.save.push(format!("{} = {source}", &c[1])),
                None => left_out = true,
            }
        } else if !(FRAME.is_match(line) || JSON_VAR.is_match(line)) {
            left_out = true;
        }
    }
    if left_out {
        notes.add("Some test script lines were left out; Crane runs no scripts, only # @expect and # @save lines", &r.name);
    }
}

/// `pm.response.json().a.b`, `jsonData.a.b` or a header read, as a save source.
fn save_source(expr: &str, json_names: &[String]) -> Option<String> {
    if let Some(c) = HEADER_GET.captures(expr) {
        return Some(format!("header {}", c[1].to_ascii_lowercase()));
    }
    let after = |prefix: &str| expr.strip_prefix(prefix).filter(|rest| rest.is_empty() || rest.starts_with(['.', '[']));
    let path = after("pm.response.json()").or_else(|| json_names.iter().find_map(|n| after(n)))?;
    let path = path.strip_prefix('.').unwrap_or(path);
    if path.is_empty() {
        return Some("json $".into());
    }
    text::json_path(path).map(|p| format!("json {p}"))
}
