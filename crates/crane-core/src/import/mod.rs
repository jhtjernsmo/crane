//! Brings requests over from Postman, Bruno and Insomnia, so moving to Crane
//! does not mean starting again.
//!
//! [`read`] turns an export into plain requests and environments without
//! touching anything; [`apply`] writes them into a collection. Secrets typed
//! straight into requests are moved to the keyring on the way, and secret
//! variables keep their values in the keyring, never in a file.

mod bruno;
mod insomnia;
mod postman;
mod text;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::collection::{Collection, slug};
use crate::env::{EnvFile, Environment, SecretStore, Var};
use crate::http_file::SavedRequest;
use crate::secret_scan::{is_secret_name, scan};
use crate::vars::is_var_name;

/// The biggest export Crane reads; real ones are far smaller.
const MAX_EXPORT_BYTES: u64 = 20 * 1024 * 1024;
/// Request names listed in a note before "and N more".
const NAMES_SHOWN: usize = 4;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct ImportError(pub String);

fn fail<T>(msg: impl Into<String>) -> Result<T, ImportError> {
    Err(ImportError(msg.into()))
}

/// What an export holds, before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    /// The collection's name, used for the folder it lands in.
    pub name: String,
    /// "Postman", "Bruno" or "Insomnia".
    pub from: &'static str,
    /// In the order the other tool runs them.
    pub requests: Vec<ImportedRequest>,
    pub environments: Vec<ImportedEnv>,
    /// Things Crane could not bring over, for people.
    pub notes: Notes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRequest {
    /// Folder names from the top of the collection.
    pub folder: Vec<String>,
    pub request: SavedRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedEnv {
    pub name: String,
    pub vars: Vec<ImportedVar>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedVar {
    pub name: String,
    /// Goes to the keyring when `secret` is set, never to a file.
    pub value: String,
    pub secret: bool,
}

/// What did not come over, grouped so one message lists every request it
/// is about instead of repeating itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notes(BTreeMap<String, Vec<String>>);

impl Notes {
    pub fn add(&mut self, what: &str, request: &str) {
        let names = self.0.entry(what.to_string()).or_default();
        if !request.is_empty() && !names.iter().any(|n| n == request) {
            names.push(request.to_string());
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|(what, names)| match names.len() {
                0 => what.clone(),
                n if n <= NAMES_SHOWN => format!("{what} ({})", names.join(", ")),
                n => format!("{what} ({} and {} more)", names[..NAMES_SHOWN].join(", "), n - NAMES_SHOWN),
            })
            .collect()
    }
}

/// Reads a Postman or Insomnia export file, or a Bruno collection folder.
pub fn read(path: &Path) -> Result<Imported, ImportError> {
    if path.is_dir() {
        return bruno::read(path).map(drop_bad_checks);
    }
    if path.extension().is_some_and(|e| e == "bru") {
        return fail("Pick the Bruno collection's folder (the one with bruno.json), not a single .bru file.");
    }
    let size = std::fs::metadata(path).map_err(|_| ImportError(format!("Could not open {}.", path.display())))?.len();
    if size > MAX_EXPORT_BYTES {
        return fail(format!("{} is {} MB, too big to be an export.", path.display(), size / 1024 / 1024));
    }
    let text = std::fs::read_to_string(path).map_err(|e| ImportError(format!("Could not read {}: {e}", path.display())))?;
    from_text(&text)
}

/// Tells the export apart by what is inside, not by its file name.
pub fn from_text(text: &str) -> Result<Imported, ImportError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("type: collection.insomnia.rest/5") || trimmed.starts_with("type: \"collection.insomnia.rest/5") {
        return fail("This is an Insomnia v5 (YAML) export, which Crane cannot read yet. Export as Insomnia v4 (JSON) instead.");
    }
    let json: serde_json::Value = serde_json::from_str(text).map_err(|_| {
        ImportError("This is not an export Crane knows. Pick a Postman collection or environment, an Insomnia export, or a Bruno folder.".into())
    })?;
    let imported = if postman::is_collection(&json) {
        postman::read_collection(&json)
    } else if postman::is_environment(&json) {
        Ok(postman::read_environment(&json))
    } else if insomnia::is_export(&json) {
        insomnia::read(&json)
    } else {
        fail("This JSON file is not a Postman collection (v2), Postman environment or Insomnia export.")
    };
    imported.map(drop_bad_checks)
}

/// A translated check Crane cannot read would fail every run, so it is
/// dropped and named instead.
fn drop_bad_checks(mut imported: Imported) -> Imported {
    for r in &mut imported.requests {
        let name = r.request.name.clone();
        let before = r.request.expect.len() + r.request.save.len();
        r.request.expect.retain(|e| crate::checks::validate(e).is_ok());
        r.request.save.retain(|v| crate::checks::validate_save(v).is_ok());
        if r.request.expect.len() + r.request.save.len() < before {
            imported.notes.add("Some checks could not be translated and were left out", &name);
        }
    }
    imported
}

/// What [`apply`] did, for showing to people.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Folder the requests went into, relative to the collection.
    pub folder: Option<String>,
    pub requests: usize,
    /// Names of the environments added.
    pub environments: Vec<String>,
    /// Secrets that were typed into requests and now live in the keyring.
    pub secrets_moved: Vec<String>,
    /// Secret variables that came without a value and still need one.
    pub secrets_to_set: Vec<String>,
    pub notes: Vec<String>,
}

/// Writes the requests into a new folder of `collection` and returns the
/// environments file with the imported environments added. Saving that file
/// is left to the caller, which also keeps its own copy up to date.
pub fn apply(imported: &Imported, collection: &Collection, envs: &EnvFile, store: &dyn SecretStore) -> Result<(EnvFile, ImportReport), ImportError> {
    let mut report = ImportReport::default();
    let mut notes = imported.notes.clone();

    let (requests, moved) = move_secrets_out(&imported.requests, &names_in_use(imported));
    let mut env_list = imported.environments.clone();
    if !moved.is_empty() && env_list.is_empty() {
        env_list.push(ImportedEnv { name: imported.name.clone(), vars: Vec::new() });
    }
    for env in &mut env_list {
        for (name, value) in &moved {
            env.vars.retain(|v| &v.name != name);
            env.vars.push(ImportedVar { name: name.clone(), value: value.clone(), secret: true });
        }
    }
    report.secrets_moved = moved.into_iter().map(|(name, _)| name).collect();

    let (next, secret_values) = add_environments(envs, &env_list, &mut notes, &mut report)?;
    if !requests.is_empty() {
        report.folder = Some(write_requests(collection, &imported.name, &requests, &mut notes)?);
        report.requests = requests.len();
    }
    for (env_id, name, value) in secret_values {
        store.set(&env_id, &name, &value).map_err(|e| ImportError(format!("Could not save a secret to the keyring: {e}")))?;
    }
    report.notes = notes.lines();
    Ok((next, report))
}

/// Replaces every literal secret with a `{{variable}}`, the same way the
/// app offers when saving. Returns the requests and (name, value) pairs.
fn move_secrets_out(requests: &[ImportedRequest], in_use: &[String]) -> (Vec<ImportedRequest>, Vec<(String, String)>) {
    let mut moved: Vec<(String, String)> = Vec::new();
    let out = requests
        .iter()
        .map(|r| {
            let mut req = r.request.clone();
            for f in scan(&req) {
                let name = match moved.iter().find(|(_, v)| *v == f.value) {
                    Some((n, _)) => n.clone(),
                    None => {
                        let n = free_name(&f.suggested_name, |n| in_use.iter().any(|u| u == n) || moved.iter().any(|(m, _)| m == n));
                        moved.push((n.clone(), f.value.clone()));
                        n
                    }
                };
                req = replace_everywhere(&req, &f.value, &format!("{{{{{name}}}}}"));
            }
            ImportedRequest { folder: r.folder.clone(), request: req }
        })
        .collect();
    (out, moved)
}

/// Variables the requests already use or the environments already have, so
/// a moved secret never takes over one of their names.
fn names_in_use(imported: &Imported) -> Vec<String> {
    let mut names: Vec<String> = imported.environments.iter().flat_map(|e| e.vars.iter().map(|v| v.name.clone())).collect();
    for r in &imported.requests {
        let q = &r.request;
        let texts = [q.url.as_str(), q.body.as_str()].into_iter().chain(q.headers.iter().map(|h| h.value.as_str()));
        names.extend(texts.flat_map(crate::vars::find_vars));
        names.extend(q.save.iter().filter_map(|s| s.split_once('=').map(|(n, _)| n.trim().to_string())));
    }
    names
}

fn replace_everywhere(r: &SavedRequest, value: &str, with: &str) -> SavedRequest {
    let mut out = r.clone();
    out.url = r.url.replace(value, with);
    out.body = r.body.replace(value, with);
    for h in &mut out.headers {
        h.value = h.value.replace(value, with);
    }
    out
}

/// `base`, then `base_2`, `base_3` until `taken` says no.
fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}_{n}")).find(|n| !taken(n)).unwrap_or_default()
}

/// (environment id, variable name, value) for the keyring.
type SecretValues = Vec<(String, String, String)>;

fn add_environments(envs: &EnvFile, list: &[ImportedEnv], notes: &mut Notes, report: &mut ImportReport) -> Result<(EnvFile, SecretValues), ImportError> {
    let mut next = envs.clone();
    let mut secrets = Vec::new();
    for env in list {
        let name = free_label(env.name.trim(), |n| next.environments.iter().any(|e| e.name.trim().eq_ignore_ascii_case(n)));
        let id = free_id(&name, |id| next.environments.iter().any(|e| e.id == id));
        let mut vars: Vec<Var> = Vec::new();
        for v in &env.vars {
            if !is_var_name(&v.name) {
                notes.add(&format!("Left out the variable \"{}\": Crane names start with a letter and use letters, digits, _, - or .", v.name), "");
                continue;
            }
            if vars.iter().any(|x| x.name == v.name) {
                continue;
            }
            let secret = v.secret || is_secret_name(&v.name);
            if secret && !v.value.is_empty() {
                secrets.push((id.clone(), v.name.clone(), v.value.clone()));
            } else if secret {
                report.secrets_to_set.push(format!("{{{{{}}}}} in {name}", v.name));
            }
            vars.push(Var { name: v.name.clone(), value: if secret { String::new() } else { v.value.clone() }, secret });
        }
        next.environments.push(Environment { id, name: name.clone(), production: false, vars, network: Default::default(), headers: vec![] });
        report.environments.push(name);
    }
    if next.active.is_none() {
        next.active = next.environments.first().map(|e| e.id.clone());
    }
    next.validate().map_err(|e| ImportError(e.to_string()))?;
    Ok((next, secrets))
}

/// `Name`, then `Name (2)` and on, for names people see.
fn free_label(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = if base.is_empty() { "Imported" } else { base };
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} ({n})")).find(|n| !taken(n)).unwrap_or_default()
}

const MAX_ID_BASE: usize = 48;

fn free_id(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let base: String = slug(name).chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').take(MAX_ID_BASE).collect();
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "imported" } else { base };
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|n| !taken(n)).unwrap_or_default()
}

/// Requests go into a new top folder named after the collection. Files and
/// folders are numbered, because a run goes in path order and should keep
/// the other tool's order.
fn write_requests(collection: &Collection, name: &str, requests: &[ImportedRequest], notes: &mut Notes) -> Result<String, ImportError> {
    let err = |e: crate::collection::CollectionError| ImportError(e.0);
    let top_name = free_label(&folder_name(name), |n| collection.root().join(n).exists());
    let top = collection.create_folder("", &top_name).map_err(err)?;

    let mut folders: BTreeMap<Vec<String>, String> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let width = requests.len().to_string().len().max(2);
    for r in requests {
        let mut parent = top.clone();
        for depth in 1..=r.folder.len() {
            let key = r.folder[..depth].to_vec();
            parent = match folders.get(&key) {
                Some(p) => p.clone(),
                None => {
                    let n = next_number(&mut counts, &parent);
                    let label = format!("{n:0width$}-{}", folder_name(&key[depth - 1]));
                    let made = collection.create_folder(&parent, &label).map_err(err)?;
                    folders.insert(key, made.clone());
                    made
                }
            };
        }
        let n = next_number(&mut counts, &parent);
        let file = format!("{parent}/{n:0width$}-{}.http", slug(&r.request.name));
        let saved = collection.save(&file, &r.request, false).map_err(err)?;
        if saved.saved.is_none() {
            notes.add("Not written, a secret is still in it", &r.request.name);
        }
    }
    Ok(top)
}

/// Folders and files share one count per folder, so a folder between two
/// requests also runs between them.
fn next_number(counts: &mut BTreeMap<String, usize>, folder: &str) -> usize {
    let n = counts.entry(folder.to_string()).or_insert(0);
    *n += 1;
    *n
}

/// Folder names cannot hold slashes or start with a dot.
fn folder_name(name: &str) -> String {
    let clean: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
    let clean = clean.trim_start_matches('.').trim().to_string();
    if clean.is_empty() { "Imported".into() } else { clean }
}

#[cfg(test)]
mod tests;
