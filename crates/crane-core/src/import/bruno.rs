//! Bruno collections: a folder with `bruno.json`, one `.bru` file per
//! request and `environments/*.bru`.
//!
//! ```text
//! meta {
//!   name: Get user
//!   seq: 2
//! }
//!
//! get {
//!   url: {{base}}/users/:id
//!   body: none
//!   auth: bearer
//! }
//! ```
//!
//! Asserts and `vars:post-response` become `# @expect` and `# @save` lines;
//! scripts and tests are left out, as Crane runs no scripts.

use std::path::Path;

use super::text::{self, Basic};
use super::{ImportError, Imported, ImportedEnv, ImportedRequest, ImportedVar, Notes};
use crate::http_file::SavedRequest;

const METHODS: [&str; 9] = ["get", "post", "put", "patch", "delete", "options", "head", "connect", "trace"];
const MAX_DEPTH: usize = 8;
const MAX_FILES: usize = 2000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const SKIPPED_DIRS: [&str; 2] = ["environments", "node_modules"];

/// One `name { ... }` or `name [ ... ]` block, its lines without the indent.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    tag: String,
    lines: Vec<String>,
}

/// Blocks open with `tag {` or `tag [` at the start of a line and close
/// with `}` or `]` alone on a line; everything inside is indented.
fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut open: Option<Block> = None;
    for line in text.replace("\r\n", "\n").lines() {
        let closing = matches!(line, "}" | "]");
        match open.as_mut() {
            Some(_) if closing => out.extend(open.take()),
            Some(b) => b.lines.push(line.strip_prefix("  ").unwrap_or(line).to_string()),
            None => {
                if let Some(tag) = line.strip_suffix(" {").or_else(|| line.strip_suffix(" [")).filter(|t| !t.starts_with(' ')) {
                    open = Some(Block { tag: tag.trim().to_string(), lines: vec![] });
                }
            }
        }
    }
    out
}

/// `key: value` lines; `~key` is switched off.
fn pairs(b: &Block) -> Vec<(String, String, bool)> {
    b.lines
        .iter()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let k = k.trim();
            let (enabled, k) = match k.strip_prefix('~') {
                Some(rest) => (false, rest),
                None => (true, k),
            };
            (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string(), enabled))
        })
        .collect()
}

fn on_pairs(b: &Block) -> Vec<(String, String)> {
    pairs(b).into_iter().filter(|(_, _, on)| *on).map(|(k, v, _)| (k, v)).collect()
}

fn get(bs: &[Block], tag: &str) -> Option<Block> {
    bs.iter().find(|b| b.tag == tag).cloned()
}

fn value(b: &Option<Block>, key: &str) -> String {
    b.as_ref().and_then(|b| on_pairs(b).into_iter().find(|(k, _)| k == key)).map(|(_, v)| v).unwrap_or_default()
}

/// Raw text blocks (bodies, scripts) keep their own indentation.
fn raw(b: &Option<Block>) -> String {
    b.as_ref().map(|b| b.lines.join("\n").trim_end().to_string()).unwrap_or_default()
}

pub fn read(dir: &Path) -> Result<Imported, ImportError> {
    let manifest = std::fs::read_to_string(dir.join("bruno.json"))
        .map_err(|_| ImportError("This folder has no bruno.json. Pick the top folder of a Bruno collection.".into()))?;
    let json: serde_json::Value = serde_json::from_str(&manifest).map_err(|_| ImportError("bruno.json in this folder is damaged.".into()))?;
    let name = json["name"].as_str().map(str::to_string).unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let mut out = Imported { name: if name.trim().is_empty() { "Bruno".into() } else { name }, from: "Bruno", ..Default::default() };

    let top = Inherited::read(&dir.join("collection.bru"), &Inherited::default(), &mut out.notes);
    let mut seen = 0;
    walk(dir, &[], &top, 0, &mut seen, &mut out)?;
    read_environments(&dir.join("environments"), &mut out);
    if out.requests.is_empty() && out.environments.is_empty() {
        return Err(ImportError("This Bruno collection has no requests.".into()));
    }
    Ok(out)
}

/// Headers and auth from `collection.bru` and `folder.bru` files, for the
/// requests below them.
#[derive(Debug, Clone, Default)]
struct Inherited {
    headers: Vec<(String, String)>,
    auth: Vec<Block>,
    mode: String,
}

impl Inherited {
    fn read(path: &Path, parent: &Inherited, notes: &mut Notes) -> Inherited {
        let Ok(text) = std::fs::read_to_string(path) else { return parent.clone() };
        let bs = blocks(&text);
        let mut headers = parent.headers.clone();
        if let Some(h) = get(&bs, "headers") {
            for (k, v) in on_pairs(&h) {
                headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&k));
                headers.push((k, v));
            }
        }
        if bs.iter().any(|b| (b.tag.starts_with("script:") || b.tag == "tests") && b.lines.iter().any(|l| !l.trim().is_empty())) {
            notes.add("Collection and folder scripts were left out; Crane runs no scripts", "");
        }
        let mode = value(&get(&bs, "auth"), "mode");
        match mode.as_str() {
            "" | "inherit" => Inherited { headers, ..parent.clone() },
            _ => Inherited { headers, auth: bs.into_iter().filter(|b| b.tag.starts_with("auth:")).collect(), mode },
        }
    }
}

fn walk(dir: &Path, folder: &[String], inherited: &Inherited, depth: usize, seen: &mut usize, out: &mut Imported) -> Result<(), ImportError> {
    if depth > MAX_DEPTH {
        out.notes.add("Folders nested deeper than 8 levels were left out", "");
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| ImportError(format!("Could not read {}: {e}", dir.display())))?;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_symlink() || name.starts_with('.') {
            continue;
        }
        if kind.is_dir() && !(depth == 0 && SKIPPED_DIRS.contains(&name.as_str())) {
            dirs.push((name, e.path()));
        } else if kind.is_file() && name.ends_with(".bru") && !matches!(name.as_str(), "collection.bru" | "folder.bru") {
            files.push(e.path());
        }
    }

    let mut requests = Vec::new();
    for path in files {
        *seen += 1;
        if *seen > MAX_FILES {
            out.notes.add("Stopped after 2000 requests", "");
            break;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            out.notes.add("Files over 1 MB were left out", &stem);
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if let Some((seq, r)) = read_request(&text, &stem, inherited, &mut out.notes) {
            requests.push((seq, r));
        }
    }
    requests.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    out.requests.extend(requests.into_iter().map(|(_, request)| ImportedRequest { folder: folder.to_vec(), request }));

    dirs.sort_by_key(|(name, path)| (folder_seq(path), name.to_lowercase()));
    for (name, path) in dirs {
        let inner_inherited = Inherited::read(&path.join("folder.bru"), inherited, &mut out.notes);
        let mut inner = folder.to_vec();
        inner.push(folder_title(&path).unwrap_or(name));
        walk(&path, &inner, &inner_inherited, depth + 1, seen, out)?;
    }
    Ok(())
}

fn folder_meta(path: &Path) -> Option<Block> {
    std::fs::read_to_string(path.join("folder.bru")).ok().and_then(|t| get(&blocks(&t), "meta"))
}

fn folder_seq(path: &Path) -> u32 {
    value(&folder_meta(path), "seq").parse().unwrap_or(u32::MAX)
}

fn folder_title(path: &Path) -> Option<String> {
    Some(value(&folder_meta(path), "name")).filter(|n| !n.is_empty())
}

fn read_request(text: &str, stem: &str, inherited: &Inherited, notes: &mut Notes) -> Option<(u32, SavedRequest)> {
    let bs = blocks(text);
    let meta = get(&bs, "meta");
    let kind = value(&meta, "type");
    if !kind.is_empty() && kind != "http" && kind != "graphql" {
        notes.add(&format!("{kind} requests were left out; Crane sends HTTP only"), stem);
        return None;
    }
    let main = bs.iter().find(|b| METHODS.contains(&b.tag.as_str())).cloned();
    let method = main.as_ref()?.tag.to_ascii_uppercase();
    let name = Some(value(&meta, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| stem.to_string());
    let seq = value(&meta, "seq").parse().unwrap_or(u32::MAX);

    let mut r = SavedRequest {
        name: name.clone(),
        method,
        url: value(&main, "url"),
        headers: get(&bs, "headers").map(|h| pairs(&h).into_iter().map(|(k, v, on)| text::header(&k, &v, on)).collect()).unwrap_or_default(),
        body_mode: "none".into(),
        body: String::new(),
        expect: vec![],
        save: vec![],
    };
    if let Some(p) = get(&bs, "params:path") {
        r.url = text::fill_path_params(&r.url, &on_pairs(&p));
    }
    read_body(&bs, &value(&main, "body"), &mut r, notes);

    let auth_mode = value(&main, "auth");
    if auth_mode == "inherit" {
        apply_auth(&inherited.mode, &inherited.auth, &mut r, notes);
    } else {
        apply_auth(&auth_mode, &bs, &mut r, notes);
    }
    for (k, v) in &inherited.headers {
        text::add_header(&mut r, k, v);
    }
    read_asserts(&bs, &mut r, notes);
    if bs.iter().any(|b| (b.tag.starts_with("script:") || b.tag == "tests") && b.lines.iter().any(|l| !l.trim().is_empty())) {
        notes.add("Scripts and tests were left out; Crane runs no scripts", &name);
    }
    if get(&bs, "vars:pre-request").is_some_and(|b| !on_pairs(&b).is_empty()) {
        notes.add("Request variables were left out; set them in an environment", &name);
    }
    if text::uses_made_up_values(&r) {
        notes.add(text::MADE_UP_VALUES, &name);
    }
    if r.url.contains("{{process.env.") || r.headers.iter().any(|h| h.value.contains("{{process.env.")) {
        notes.add("Uses {{process.env.X}}; add X to an environment and write {{X}} instead", &name);
    }
    Some((seq, r))
}

fn read_body(bs: &[Block], mode: &str, r: &mut SavedRequest, notes: &mut Notes) {
    match mode {
        "json" => {
            r.body_mode = "json".into();
            r.body = raw(&get(bs, "body:json"));
        }
        "text" => {
            r.body_mode = "text".into();
            r.body = raw(&get(bs, "body:text"));
        }
        "xml" => {
            r.body_mode = "text".into();
            r.body = raw(&get(bs, "body:xml"));
            text::add_header(r, "Content-Type", "application/xml");
        }
        "formUrlEncoded" => {
            r.body_mode = "form".into();
            r.body = text::form_body(&get(bs, "body:form-urlencoded").map(|b| on_pairs(&b)).unwrap_or_default());
        }
        "multipartForm" => {
            let fields = get(bs, "body:multipart-form").map(|b| on_pairs(&b)).unwrap_or_default();
            if fields.iter().any(|(_, v)| v.starts_with("@file(")) {
                notes.add("File uploads were left out; Crane cannot send files yet", &r.name);
            }
            let fields: Vec<(String, String)> = fields.into_iter().filter(|(_, v)| !v.starts_with("@file(")).collect();
            r.body_mode = "form".into();
            r.body = text::form_body(&fields);
            notes.add("Multipart forms are sent URL-encoded; Crane has no multipart yet", &r.name);
        }
        "graphql" => {
            let vars: serde_json::Value = serde_json::from_str(&raw(&get(bs, "body:graphql:vars"))).unwrap_or(serde_json::Value::Null);
            let doc = serde_json::json!({ "query": raw(&get(bs, "body:graphql")), "variables": vars });
            r.body_mode = "json".into();
            r.body = serde_json::to_string_pretty(&doc).unwrap_or_default();
        }
        "" | "none" => {}
        other => notes.add(&format!("\"{other}\" bodies were left out"), &r.name),
    }
}

fn apply_auth(mode: &str, bs: &[Block], r: &mut SavedRequest, notes: &mut Notes) {
    let block = |tag: &str| get(bs, tag);
    match mode {
        "" | "none" | "inherit" => {}
        "bearer" => text::add_header(r, "Authorization", &format!("Bearer {}", value(&block("auth:bearer"), "token"))),
        "basic" => {
            let b = block("auth:basic");
            match text::basic_auth(&value(&b, "username"), &value(&b, "password")) {
                Basic::Header(h) => text::add_header(r, "Authorization", &h),
                Basic::HasVariables => notes.add(text::BASIC_WITH_VARS, &r.name),
            }
        }
        "apikey" => {
            let b = block("auth:apikey");
            let (key, val) = (value(&b, "key"), value(&b, "value"));
            if value(&b, "placement") == "queryparams" {
                r.url = text::with_query(&r.url, &[(key, val)]);
            } else if !key.is_empty() {
                text::add_header(r, &key, &val);
            }
        }
        other => notes.add(&format!("\"{other}\" auth was left out; set the header it makes by hand"), &r.name),
    }
}

/// `res.body.a.b` as `json $.a.b`, `res.headers.x` as `header x`.
fn source(expr: &str) -> Option<String> {
    let expr = expr.trim();
    match expr {
        "res.status" => return Some("status".into()),
        "res.responseTime" => return Some("time".into()),
        "res.body" => return Some("body".into()),
        _ => {}
    }
    if let Some(h) = expr.strip_prefix("res.headers.").or_else(|| expr.strip_prefix("res.headers[")) {
        let h = h.trim_matches(|c| matches!(c, '"' | '\'' | ']'));
        return Some(format!("header {}", h.to_ascii_lowercase()));
    }
    let path = expr.strip_prefix("res.body").filter(|p| p.starts_with(['.', '[']))?;
    text::json_path(path.strip_prefix('.').unwrap_or(path)).map(|p| format!("json {p}"))
}

fn read_asserts(bs: &[Block], r: &mut SavedRequest, notes: &mut Notes) {
    for (expr, test) in get(bs, "assert").map(|b| on_pairs(&b)).unwrap_or_default() {
        let (op, arg) = test.split_once(' ').map(|(o, a)| (o, a.trim())).unwrap_or((test.as_str(), ""));
        let line = source(&expr).and_then(|src| {
            let unit = if src == "time" { "ms" } else { "" };
            Some(match (src.as_str(), op) {
                ("status", "eq") => format!("status {arg}"),
                (_, "eq") => format!("{src} = {arg}"),
                (_, "neq") => format!("{src} != {arg}"),
                (_, "contains") => format!("{src} contains {arg}"),
                (_, "lt") => format!("{src} < {arg}{unit}"),
                (_, "gt") => format!("{src} > {arg}{unit}"),
                (_, "isDefined") => format!("{src} exists"),
                (_, "length") => format!("{src} length {arg}"),
                _ => return None,
            })
        });
        match line {
            Some(l) => r.expect.push(l),
            None => notes.add("Some asserts were left out; Crane has no check like them yet", &r.name),
        }
    }
    for (name, expr) in get(bs, "vars:post-response").map(|b| on_pairs(&b)).unwrap_or_default() {
        match source(&expr).filter(|s| s.starts_with("json") || s.starts_with("header")) {
            Some(src) => r.save.push(format!("{name} = {src}")),
            None => notes.add("Some post-response variables were left out", &r.name),
        }
    }
}

fn read_environments(dir: &Path, out: &mut Imported) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "bru")).collect();
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let bs = blocks(&text);
        let mut vars: Vec<ImportedVar> =
            get(&bs, "vars").map(|b| on_pairs(&b).into_iter().map(|(name, value)| ImportedVar { name, value, secret: false }).collect()).unwrap_or_default();
        // Bruno keeps secret values on the computer, never in the file.
        for name in get(&bs, "vars:secret").map(|b| b.lines).unwrap_or_default() {
            let name = name.trim().trim_end_matches(',').trim().trim_start_matches('~').to_string();
            if !name.is_empty() {
                vars.retain(|v| v.name != name);
                vars.push(ImportedVar { name, value: String::new(), secret: true });
            }
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        out.environments.push(ImportedEnv { name, vars });
    }
}
