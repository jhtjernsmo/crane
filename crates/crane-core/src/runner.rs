//! Runs the requests in a collection, one after another, and checks their
//! answers. Used by the app's "Run all" and by `crane-cli` in CI.
//!
//! Variables come from the environment (secrets from the store) and from
//! `@save` lines of earlier requests in the same run, which count as
//! secrets: they are hidden in answers, errors and check details.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

use crate::checks::{CheckResult, check, save};
use crate::collection::{Collection, Node};
use crate::env::{Environment, SecretError, SecretStore, Var, resolve};
use crate::http_file::to_spec;
use crate::vars::{mask, substitute};
use crate::{SendOptions, send};

/// Status codes from here on count as failures for requests without checks.
const FAILING_STATUS: u16 = 400;
/// The environment id used for a run without an environment.
const NO_ENV: &str = "run";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub path: String,
    pub name: String,
    pub method: String,
    pub status: Option<u16>,
    pub total_ms: Option<u64>,
    /// Why it could not be sent or read, if it could not.
    pub error: Option<String>,
    pub checks: Vec<CheckResult>,
    /// Names of the variables it saved for later requests.
    pub saved: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub outcomes: Vec<Outcome>,
    pub elapsed_ms: u64,
}

impl Report {
    pub fn failed(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.passed).count()
    }
    pub fn passed(&self) -> usize {
        self.outcomes.len() - self.failed()
    }
}

pub struct RunOptions<'a> {
    pub env: Option<Environment>,
    pub store: &'a dyn SecretStore,
    /// Values given for this run (for example from CI), treated as secrets.
    pub vars: Vec<(String, String)>,
    pub send: SendOptions,
    /// Stop at the first failure.
    pub bail: bool,
}

/// Every request in the collection, in path order, so `01-login.http`
/// runs before `users/get-me.http`: name files to set the order.
pub fn request_paths(nodes: &[Node]) -> Vec<String> {
    fn collect(nodes: &[Node], out: &mut Vec<String>) {
        for n in nodes {
            match n {
                Node::Folder { children, .. } => collect(children, out),
                Node::Request { path, .. } => out.push(path.clone()),
            }
        }
    }
    let mut out = Vec::new();
    collect(nodes, &mut out);
    out.sort();
    out
}

/// Run values first, then the real store.
struct RunStore<'a> {
    run: Mutex<HashMap<String, String>>,
    inner: &'a dyn SecretStore,
}

impl SecretStore for RunStore<'_> {
    fn get(&self, env_id: &str, name: &str) -> Result<Option<String>, SecretError> {
        if let Some(v) = self.run.lock().map_err(|_| SecretError("lock".into()))?.get(name) {
            return Ok(Some(v.clone()));
        }
        self.inner.get(env_id, name)
    }
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError("a run does not store secrets".into()))
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

/// The environment plus this run's values, which override and are secret.
fn with_run_vars(env: &Option<Environment>, run: &HashMap<String, String>) -> Environment {
    let mut e = env.clone().unwrap_or(Environment { id: NO_ENV.into(), name: "no environment".into(), production: false, vars: vec![], network: Default::default() });
    for name in run.keys() {
        e.vars.retain(|v| &v.name != name);
        e.vars.push(Var { name: name.clone(), value: String::new(), secret: true });
    }
    e
}

pub async fn run(col: &Collection, paths: &[String], opts: RunOptions<'_>, mut on_outcome: impl FnMut(&Outcome)) -> Report {
    let started = Instant::now();
    let store = RunStore { run: Mutex::new(opts.vars.iter().cloned().collect()), inner: opts.store };
    let mut outcomes = Vec::new();
    for path in paths {
        let run_vars = store.run.lock().map(|m| m.clone()).unwrap_or_default();
        let env = with_run_vars(&opts.env, &run_vars);
        let outcome = run_one(col, path, &env, &store, &run_vars, &opts.send).await;
        let saved: Vec<(String, String)> = outcome.1;
        if let Ok(mut m) = store.run.lock() {
            m.extend(saved);
        }
        on_outcome(&outcome.0);
        let failed = !outcome.0.passed;
        outcomes.push(outcome.0);
        if failed && opts.bail {
            break;
        }
    }
    Report { outcomes, elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX) }
}

async fn run_one(
    col: &Collection,
    path: &str,
    env: &Environment,
    store: &RunStore<'_>,
    run_vars: &HashMap<String, String>,
    base: &SendOptions,
) -> (Outcome, Vec<(String, String)>) {
    let mut out = Outcome { path: path.into(), name: path.into(), method: String::new(), status: None, total_ms: None, error: None, checks: vec![], saved: vec![], passed: false };
    let saved_req = match col.read(path) {
        Ok(r) => r.request,
        Err(e) => {
            out.error = Some(e.to_string());
            return (out, vec![]);
        }
    };
    out.name = saved_req.name.clone();
    out.method = saved_req.method.clone();
    let resolved = match resolve(&to_spec(&saved_req), Some(env), store) {
        Ok(r) => r,
        Err(e) => {
            out.error = Some(e.to_string());
            return (out, vec![]);
        }
    };
    // Hide every run value, not only the ones this request used.
    let mut secrets = resolved.secrets.clone();
    secrets.extend(run_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
    let opts = SendOptions { network: resolved.network.clone(), ..base.clone() };
    let res = match send(&resolved.spec, &opts).await {
        Ok(r) => r,
        Err(e) => {
            out.error = Some(mask(&e.to_string(), &secrets));
            return (out, vec![]);
        }
    };
    out.status = Some(res.status);
    out.total_ms = Some(res.timing.total_ms);
    out.checks = run_checks(&saved_req.expect, &res, &env_values(env, store), &secrets);
    let mut kept = Vec::new();
    for line in &saved_req.save {
        match save(line, &res) {
            Ok((name, value)) => {
                out.saved.push(name.clone());
                kept.push((name, value));
            }
            Err(e) => out.checks.push(CheckResult { check: format!("save {line}"), passed: false, detail: Some(e) }),
        }
    }
    out.passed = if out.checks.is_empty() { res.status < FAILING_STATUS } else { out.checks.iter().all(|c| c.passed) };
    (out, kept)
}

/// Runs `@expect` lines against an answer that still has its secrets in it.
/// Lines can use variables (`json $.id = {{user_id}}`); results show the
/// line as written and hide secrets in the details.
pub fn run_checks(lines: &[String], res: &crate::Response, values: &HashMap<String, String>, secrets: &[(String, String)]) -> Vec<CheckResult> {
    lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let filled = substitute(line, &|n: &str| values.get(n).cloned()).unwrap_or_else(|_| line.clone());
            let mut c = check(&filled, res);
            c.check = line.clone();
            c.detail = c.detail.map(|d| mask(&d, secrets));
            c
        })
        .collect()
}

/// The value of every variable in the environment (and this run), for
/// filling in check lines.
pub fn env_values(env: &Environment, store: &dyn SecretStore) -> HashMap<String, String> {
    env.vars
        .iter()
        .filter_map(|v| {
            let value = if v.secret { store.get(&env.id, &v.name).ok().flatten()? } else { v.value.clone() };
            Some((v.name.clone(), value))
        })
        .collect()
}
