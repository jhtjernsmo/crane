//! Environments: named sets of variables like `base` and `token`, so the same
//! request can go to Local, Test or Prod.
//!
//! Plain values live in `environments.json`. Secret values never do: they
//! live in a [`SecretStore`] (the system keyring in the app), are read only
//! by Rust right before sending, and are hidden again in what comes back.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::network::Network;
use crate::vars::{find_vars, is_var_name, substitute};
use crate::{Header, RequestSpec, SendError};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvFile {
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub environments: Vec<Environment>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: String,
    pub name: String,
    /// Crane asks before sending anything that changes data here.
    #[serde(default)]
    pub production: bool,
    #[serde(default)]
    pub vars: Vec<Var>,
    #[serde(default)]
    pub network: Network,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Var {
    pub name: String,
    /// Always empty for secrets; their value is in the secret store.
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvError {
    #[error("{0}")]
    Invalid(String),
    #[error("Could not read or save the environments file: {0}")]
    Io(String),
}

const MAX_ID_LEN: usize = 64;

impl EnvFile {
    /// A missing file is the same as no environments yet.
    pub fn load(path: &Path) -> Result<Self, EnvError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| EnvError::Io(format!("{} is damaged ({e})", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(EnvError::Io(e.to_string())),
        }
    }

    /// Writes to a temporary file first, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<(), EnvError> {
        let io = |e: std::io::Error| EnvError::Io(e.to_string());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let text = serde_json::to_string_pretty(&self.without_secret_values()).map_err(|e| EnvError::Io(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text + "\n").map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
        }
        std::fs::rename(&tmp, path).map_err(io)
    }

    /// A copy where no secret carries a value, whatever the caller sent.
    pub fn without_secret_values(&self) -> Self {
        let mut copy = self.clone();
        for v in copy.environments.iter_mut().flat_map(|e| e.vars.iter_mut()).filter(|v| v.secret) {
            v.value.clear();
        }
        copy
    }

    pub fn get(&self, id: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.id == id)
    }

    /// Checks names before saving, with messages that say which one is wrong.
    pub fn validate(&self) -> Result<(), EnvError> {
        let bad = |m: String| Err(EnvError::Invalid(m));
        let mut ids = Vec::new();
        let mut names = Vec::new();
        for env in &self.environments {
            let id_ok = !env.id.is_empty() && env.id.len() <= MAX_ID_LEN && env.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !id_ok || ids.contains(&&env.id) {
                return bad(format!("The environment \"{}\" has a broken id. Remove it and add it again.", env.name));
            }
            ids.push(&env.id);
            let name = env.name.trim().to_lowercase();
            if name.is_empty() {
                return bad("Every environment needs a name.".into());
            }
            if names.contains(&name) {
                return bad(format!("There are two environments called \"{}\".", env.name.trim()));
            }
            names.push(name);
            env.network.validate(env.name.trim(), env.production).map_err(EnvError::Invalid)?;
            let mut var_names: Vec<&str> = Vec::new();
            for v in &env.vars {
                if !is_var_name(&v.name) {
                    return bad(format!(
                        "\"{}\" in {} is not a usable name. Start with a letter, then use letters, digits, _, - or .",
                        v.name,
                        env.name.trim()
                    ));
                }
                if var_names.contains(&v.name.as_str()) {
                    return bad(format!("{} has {{{{{}}}}} twice.", env.name.trim(), v.name));
                }
                var_names.push(&v.name);
            }
        }
        if let Some(active) = &self.active
            && self.get(active).is_none()
        {
            return bad("The picked environment does not exist any more.".into());
        }
        Ok(())
    }

    /// Secrets in `self` that `next` no longer has as secrets, so their stored
    /// values can be deleted instead of lingering in the keyring.
    pub fn secrets_dropped_by(&self, next: &EnvFile) -> Vec<(String, String)> {
        let still_secret = |env_id: &str, name: &str| next.get(env_id).is_some_and(|e| e.vars.iter().any(|v| v.secret && v.name == name));
        self.environments
            .iter()
            .flat_map(|e| e.vars.iter().filter(|v| v.secret).map(move |v| (e.id.clone(), v.name.clone())))
            .filter(|(id, name)| !still_secret(id, name))
            .collect()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct SecretError(pub String);

/// Where secret values are kept. The app uses the system keyring; tests use
/// [`MemorySecrets`].
pub trait SecretStore: Send + Sync {
    fn get(&self, env_id: &str, name: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, env_id: &str, name: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, env_id: &str, name: &str) -> Result<(), SecretError>;
}

#[derive(Default)]
pub struct MemorySecrets(std::sync::Mutex<HashMap<(String, String), String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, env_id: &str, name: &str) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().expect("lock").get(&(env_id.into(), name.into())).cloned())
    }
    fn set(&self, env_id: &str, name: &str, value: &str) -> Result<(), SecretError> {
        self.0.lock().expect("lock").insert((env_id.into(), name.into()), value.into());
        Ok(())
    }
    fn delete(&self, env_id: &str, name: &str) -> Result<(), SecretError> {
        self.0.lock().expect("lock").remove(&(env_id.into(), name.into()));
        Ok(())
    }
}

/// A request with its variables filled in, plus the secret values used, so
/// they can be hidden in the response.
#[derive(Debug)]
pub struct Resolved {
    pub spec: RequestSpec,
    pub secrets: Vec<(String, String)>,
    /// The environment's network settings, with the proxy's variables filled in.
    pub network: Network,
}

fn used_vars(spec: &RequestSpec, network: &Network) -> Vec<String> {
    let mut texts: Vec<&str> = vec![&spec.url, &network.proxy];
    for h in &spec.headers {
        texts.push(&h.name);
        texts.push(&h.value);
    }
    if let Some(b) = &spec.body {
        texts.push(b);
    }
    let mut names: Vec<String> = Vec::new();
    for n in texts.into_iter().flat_map(find_vars) {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    names
}

/// Fills in `{{variables}}` from `env`, reading secrets from `store`.
pub fn resolve(spec: &RequestSpec, env: Option<&Environment>, store: &dyn SecretStore) -> Result<Resolved, SendError> {
    let network = env.map(|e| e.network.clone()).unwrap_or_default();
    let names = used_vars(spec, &network);
    if names.is_empty() {
        return Ok(Resolved { spec: spec.clone(), secrets: Vec::new(), network });
    }
    let Some(env) = env else { return Err(SendError::NoEnvironment(names)) };

    let mut values: HashMap<String, String> = HashMap::new();
    let mut secrets = Vec::new();
    let mut missing = Vec::new();
    for name in names {
        let Some(var) = env.vars.iter().find(|v| v.name == name) else {
            missing.push(name);
            continue;
        };
        if var.secret {
            let value =
                store.get(&env.id, &name).map_err(|e| SendError::Keyring(e.0))?.ok_or_else(|| SendError::SecretNotSet(env.name.clone(), name.clone()))?;
            secrets.push((name.clone(), value.clone()));
            values.insert(name, value);
        } else {
            values.insert(name, var.value.clone());
        }
    }
    if !missing.is_empty() {
        return Err(SendError::MissingVars(env.name.clone(), missing));
    }

    let lookup = |n: &str| values.get(n).cloned();
    // Every name was found above, so filling in cannot fail here.
    let fill = |text: &str| substitute(text, &lookup).unwrap_or_else(|_| text.to_string());
    let spec = RequestSpec {
        method: spec.method.clone(),
        url: fill(&spec.url),
        headers: spec.headers.iter().map(|h| Header { name: fill(&h.name), value: fill(&h.value) }).collect(),
        body: spec.body.as_deref().map(fill),
    };
    let network = Network { proxy: fill(&network.proxy), ..network };
    Ok(Resolved { spec, secrets, network })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, value: &str) -> Var {
        Var { name: name.into(), value: value.into(), secret: false }
    }
    fn secret(name: &str) -> Var {
        Var { name: name.into(), value: String::new(), secret: true }
    }
    fn env(id: &str, name: &str, vars: Vec<Var>) -> Environment {
        Environment { id: id.into(), name: name.into(), production: false, vars, network: Network::default() }
    }
    fn spec(url: &str) -> RequestSpec {
        RequestSpec { method: "GET".into(), url: url.into(), headers: vec![], body: None }
    }

    #[test]
    fn fills_url_headers_and_body_and_reports_secrets_used() {
        let store = MemorySecrets::default();
        store.set("local", "token", "s3cret-value").unwrap();
        let e = env("local", "Local", vec![var("base", "localhost:8080"), secret("token")]);
        let mut s = spec("{{base}}/me");
        s.headers = vec![Header { name: "Authorization".into(), value: "Bearer {{token}}".into() }];
        s.body = Some(r#"{"from":"{{base}}"}"#.into());

        let r = resolve(&s, Some(&e), &store).unwrap();
        assert_eq!(r.spec.url, "localhost:8080/me");
        assert_eq!(r.spec.headers[0].value, "Bearer s3cret-value");
        assert_eq!(r.spec.body.as_deref(), Some(r#"{"from":"localhost:8080"}"#));
        assert_eq!(r.secrets, [("token".to_string(), "s3cret-value".to_string())]);
    }

    #[test]
    fn requests_without_variables_need_no_environment() {
        let r = resolve(&spec("x.dev"), None, &MemorySecrets::default()).unwrap();
        assert_eq!(r.spec.url, "x.dev");
    }

    #[test]
    fn says_what_is_missing() {
        let store = MemorySecrets::default();
        assert_eq!(resolve(&spec("{{base}}"), None, &store).unwrap_err(), SendError::NoEnvironment(vec!["base".into()]));

        let e = env("local", "Local", vec![var("base", "x"), secret("token")]);
        let err = resolve(&spec("{{base}}/{{a}}/{{b}}"), Some(&e), &store).unwrap_err();
        assert_eq!(err, SendError::MissingVars("Local".into(), vec!["a".into(), "b".into()]));
        assert_eq!(err.to_string(), "Local has no {{a}} or {{b}}. Add them under Environments, or pick another environment.");

        let err = resolve(&spec("{{token}}"), Some(&e), &store).unwrap_err();
        assert_eq!(err.to_string(), "The secret {{token}} in Local has no value yet. Set it under Environments.");
    }

    #[test]
    fn saved_files_never_hold_secret_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crane/environments.json");
        let mut leaky = secret("token");
        leaky.value = "should-not-be-saved".into();
        let file = EnvFile { active: Some("local".into()), environments: vec![env("local", "Local", vec![var("base", "x"), leaky])] };

        file.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("should-not-be-saved"));
        let back = EnvFile::load(&path).unwrap();
        assert_eq!(back.environments[0].vars[1], secret("token"));
        assert_eq!(back.active.as_deref(), Some("local"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_missing_file_means_no_environments() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(EnvFile::load(&dir.path().join("none.json")).unwrap(), EnvFile::default());
    }

    #[test]
    fn a_damaged_file_is_reported_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.json");
        std::fs::write(&path, "{ nope").unwrap();
        assert!(matches!(EnvFile::load(&path), Err(EnvError::Io(m)) if m.contains("damaged")));
    }

    #[test]
    fn validation_names_the_problem() {
        let check = |envs: Vec<Environment>| EnvFile { active: None, environments: envs }.validate();
        assert!(check(vec![env("a", "Local", vec![var("base", "")]), env("b", "Prod", vec![])]).is_ok());
        assert_eq!(
            check(vec![env("a", "Local", vec![]), env("b", " local ", vec![])]),
            Err(EnvError::Invalid("There are two environments called \"local\".".into()))
        );
        assert!(check(vec![env("a", "Local", vec![var("2fa", "")])]).unwrap_err().to_string().contains("\"2fa\" in Local"));
        assert_eq!(check(vec![env("a", "Local", vec![var("x", ""), secret("x")])]), Err(EnvError::Invalid("Local has {{x}} twice.".into())));
        assert!(check(vec![env("BAD ID", "Local", vec![])]).is_err());
        assert!(EnvFile { active: Some("gone".into()), environments: vec![] }.validate().is_err());
    }

    #[test]
    fn knows_which_secrets_to_delete_from_the_keyring() {
        let old = EnvFile {
            active: None,
            environments: vec![env("a", "Local", vec![secret("token"), secret("renamed"), secret("unmarked")]), env("b", "Prod", vec![secret("token")])],
        };
        let new = EnvFile { active: None, environments: vec![env("a", "Local", vec![secret("token"), secret("new_name"), var("unmarked", "")])] };
        assert_eq!(old.secrets_dropped_by(&new), [("a".into(), "renamed".into()), ("a".into(), "unmarked".into()), ("b".into(), "token".into())]);
    }
}
