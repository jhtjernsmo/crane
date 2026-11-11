//! Insomnia v4 exports (JSON): a flat list of resources that point to their
//! parent, `{{ _.name }}` variables and environments nested under a base.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::text::{self, Basic};
use super::{ImportError, Imported, ImportedEnv, ImportedRequest, ImportedVar, Notes};
use crate::http_file::SavedRequest;

const MAX_DEPTH: usize = 16;

pub fn is_export(json: &Value) -> bool {
    json["_type"] == "export" && json["resources"].is_array()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_default()
}

static VAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*(?:_\.)?([A-Za-z_][\w.-]*)\s*\}\}").expect("valid pattern"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{%.*?%\}").expect("valid pattern"));

/// `{{ _.base }}` and `{{base}}` both become Crane's `{{base}}`.
fn vars(text: &str) -> String {
    VAR.replace_all(text, "{{$1}}").into_owned()
}

pub fn read(json: &Value) -> Result<Imported, ImportError> {
    let resources: Vec<&Value> = json["resources"].as_array().into_iter().flatten().collect();
    let workspace = resources.iter().find(|r| s(r, "_type") == "workspace");
    let name = workspace.map(|w| s(w, "name")).filter(|n| !n.is_empty()).unwrap_or("Insomnia");
    let mut out = Imported { name: name.to_string(), from: "Insomnia", ..Default::default() };

    let roots: Vec<String> = match workspace {
        Some(w) => vec![s(w, "_id").to_string()],
        None => {
            let mut parents: Vec<String> =
                resources.iter().map(|r| s(r, "parentId").to_string()).filter(|p| !resources.iter().any(|r| s(r, "_id") == p)).collect();
            parents.sort();
            parents.dedup();
            parents
        }
    };
    for root in &roots {
        walk(&resources, root, &[], 0, &mut out);
    }
    read_environments(&resources, &roots, &mut out);
    if out.requests.is_empty() && out.environments.is_empty() {
        return Err(ImportError("This Insomnia export has no requests.".into()));
    }
    Ok(out)
}

/// Children in the order Insomnia shows them.
fn children<'a>(resources: &[&'a Value], parent: &str) -> Vec<&'a Value> {
    let mut list: Vec<&Value> = resources.iter().copied().filter(|r| s(r, "parentId") == parent).collect();
    list.sort_by(|a, b| a["metaSortKey"].as_f64().unwrap_or(0.0).total_cmp(&b["metaSortKey"].as_f64().unwrap_or(0.0)));
    list
}

fn walk(resources: &[&Value], parent: &str, folder: &[String], depth: usize, out: &mut Imported) {
    if depth > MAX_DEPTH {
        out.notes.add("Folders nested deeper than 16 levels were left out", "");
        return;
    }
    for r in children(resources, parent) {
        match s(r, "_type") {
            "request_group" => {
                let mut inner = folder.to_vec();
                inner.push(Some(s(r, "name")).filter(|n| !n.is_empty()).unwrap_or("Folder").to_string());
                walk(resources, s(r, "_id"), &inner, depth + 1, out);
            }
            "request" => {
                let request = read_request(r, &mut out.notes);
                out.requests.push(ImportedRequest { folder: folder.to_vec(), request });
            }
            "grpc_request" | "websocket_request" => out.notes.add("gRPC and WebSocket requests were left out; Crane sends HTTP only", s(r, "name")),
            _ => {}
        }
    }
}

fn on(v: &Value) -> bool {
    !v["disabled"].as_bool().unwrap_or(false)
}

fn named_pairs(list: &Value) -> Vec<(String, String)> {
    list.as_array().into_iter().flatten().filter(|p| on(p) && !s(p, "name").is_empty()).map(|p| (vars(s(p, "name")), vars(s(p, "value")))).collect()
}

fn read_request(r: &Value, notes: &mut Notes) -> SavedRequest {
    let name = Some(s(r, "name")).filter(|n| !n.is_empty()).unwrap_or("Request").to_string();
    let mut req = SavedRequest {
        name: name.clone(),
        method: Some(s(r, "method").to_ascii_uppercase()).filter(|m| !m.is_empty()).unwrap_or_else(|| "GET".into()),
        url: text::with_query(&vars(s(r, "url")), &named_pairs(&r["parameters"])),
        headers: r["headers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|h| !s(h, "name").is_empty())
            .map(|h| text::header(&vars(s(h, "name")), &vars(s(h, "value")), on(h)))
            .collect(),
        body_mode: "none".into(),
        body: String::new(),
        expect: vec![],
        save: vec![],
    };
    read_body(&r["body"], &mut req, notes);
    read_auth(&r["authentication"], &mut req, notes);
    if [&req.url, &req.body].iter().any(|t| TAG.is_match(t)) || req.headers.iter().any(|h| TAG.is_match(&h.value)) {
        notes.add("Template tags like {% response %} or {% uuid %} were kept as text; Crane has no tags, so replace them with variables", &name);
    }
    if !s(r, "preRequestScript").trim().is_empty() || !s(r, "afterResponseScript").trim().is_empty() {
        notes.add("Scripts were left out; Crane runs no scripts", &name);
    }
    req
}

fn read_body(body: &Value, r: &mut SavedRequest, notes: &mut Notes) {
    let mime = s(body, "mimeType");
    let body_text = vars(s(body, "text"));
    match mime {
        "" if body_text.is_empty() => {}
        "application/x-www-form-urlencoded" => {
            r.body_mode = "form".into();
            r.body = text::form_body(&named_pairs(&body["params"]));
        }
        "multipart/form-data" => {
            let params = body["params"].as_array().cloned().unwrap_or_default();
            if params.iter().any(|p| s(p, "type") == "file") {
                notes.add("File uploads were left out; Crane cannot send files yet", &r.name);
            }
            let fields: Vec<(String, String)> = params
                .iter()
                .filter(|p| on(p) && s(p, "type") != "file" && !s(p, "name").is_empty())
                .map(|p| (vars(s(p, "name")), vars(s(p, "value"))))
                .collect();
            r.body_mode = "form".into();
            r.body = text::form_body(&fields);
            notes.add("Multipart forms are sent URL-encoded; Crane has no multipart yet", &r.name);
        }
        "application/graphql" => {
            // Insomnia already stores {"query": ..., "variables": ...} as the text.
            r.body_mode = "json".into();
            r.body = body_text;
        }
        "application/octet-stream" => notes.add("File bodies were left out; Crane cannot send files yet", &r.name),
        m if m == "application/json" || m.ends_with("+json") => {
            r.body_mode = "json".into();
            r.body = body_text;
            if m != "application/json" {
                text::add_header(r, "Content-Type", m);
            }
        }
        m => {
            r.body_mode = "text".into();
            r.body = body_text;
            if !m.is_empty() && m != "text/plain" {
                text::add_header(r, "Content-Type", m);
            }
        }
    }
}

fn read_auth(auth: &Value, r: &mut SavedRequest, notes: &mut Notes) {
    if auth["disabled"].as_bool().unwrap_or(false) {
        return;
    }
    match s(auth, "type") {
        "" | "none" => {}
        "bearer" => {
            let prefix = Some(s(auth, "prefix")).filter(|p| !p.is_empty()).unwrap_or("Bearer");
            text::add_header(r, "Authorization", &format!("{prefix} {}", vars(s(auth, "token"))));
        }
        "basic" => match text::basic_auth(&vars(s(auth, "username")), &vars(s(auth, "password"))) {
            Basic::Header(h) => text::add_header(r, "Authorization", &h),
            Basic::HasVariables => notes.add(text::BASIC_WITH_VARS, &r.name),
        },
        "apikey" => {
            let (key, value) = (vars(s(auth, "key")), vars(s(auth, "value")));
            if s(auth, "addTo") == "queryParams" {
                r.url = text::with_query(&r.url, &[(key, value)]);
            } else if !key.is_empty() {
                text::add_header(r, &key, &value);
            }
        }
        other => notes.add(&format!("\"{other}\" auth was left out; set the header it makes by hand"), &r.name),
    }
}

/// `{"a": {"b": 1}}` as `a.b = 1`, the way `{{ _.a.b }}` reads it.
fn flatten(prefix: &str, v: &Value, out: &mut Vec<ImportedVar>) {
    match v {
        Value::Object(map) => {
            for (k, inner) in map {
                let name = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(&name, inner, out);
            }
        }
        Value::String(s) => out.push(ImportedVar { name: prefix.to_string(), value: vars(s), secret: false }),
        Value::Null => {}
        other => out.push(ImportedVar { name: prefix.to_string(), value: other.to_string(), secret: false }),
    }
}

fn env_vars(env: &Value) -> Vec<ImportedVar> {
    let mut out = Vec::new();
    flatten("", &env["data"], &mut out);
    out
}

/// Each sub-environment becomes a Crane environment with the base values
/// under its own. A base without sub-environments becomes one by itself.
fn read_environments(resources: &[&Value], roots: &[String], out: &mut Imported) {
    for base in resources.iter().filter(|r| s(r, "_type") == "environment" && roots.iter().any(|root| root == s(r, "parentId"))) {
        let base_vars = env_vars(base);
        let subs: Vec<&Value> = children(resources, s(base, "_id")).into_iter().filter(|r| s(r, "_type") == "environment").collect();
        if subs.is_empty() {
            if !base_vars.is_empty() {
                let name = Some(s(base, "name")).filter(|n| !n.is_empty()).unwrap_or("Base Environment");
                out.environments.push(ImportedEnv { name: name.to_string(), vars: base_vars });
            }
            continue;
        }
        for sub in subs {
            let mut merged = base_vars.clone();
            for v in env_vars(sub) {
                merged.retain(|m| m.name != v.name);
                merged.push(v);
            }
            let name = Some(s(sub, "name")).filter(|n| !n.is_empty()).unwrap_or("Environment");
            out.environments.push(ImportedEnv { name: name.to_string(), vars: merged });
        }
    }
}
