//! Rules an organisation can set for every Crane on its machines, in a file
//! only administrators can write:
//!
//! - Linux: `/etc/crane/policy.toml`
//! - macOS: `/Library/Application Support/Crane/policy.toml`
//! - Windows: `%ProgramData%\Crane\policy.toml`
//!
//! ```toml
//! # Who to ask about these rules, shown when something is refused.
//! contact = "it-security@example.com"
//! # Only these hosts may be sent to (also after redirects).
//! allowed_hosts = ["*.example.com", "localhost", "127.0.0.1"]
//! blocked_hosts = ["admin.example.com"]
//! allow_saving_secrets = false   # no "save with the secret anyway"
//! allow_insecure_tls = false     # no skipping certificate checks
//! containers = false             # no container engine, no stacks
//! keep_history = false           # nothing written to history.jsonl
//! check_updates = false          # no update checks; IT ships new versions
//! ```
//!
//! Every rule is enforced in Rust, so the UI cannot get around it. A policy
//! file that cannot be read means the strictest rules, not none.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Read from TOML with snake_case keys; sent to the UI in camelCase like
/// everything else.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default, rename_all(serialize = "camelCase"))]
pub struct Policy {
    pub contact: Option<String>,
    /// `None` allows every host. Patterns: `api.example.com`, `*.example.com`.
    pub allowed_hosts: Option<Vec<String>>,
    pub blocked_hosts: Vec<String>,
    pub allow_saving_secrets: bool,
    pub allow_insecure_tls: bool,
    pub containers: bool,
    pub keep_history: bool,
    /// Ask GitHub for a newer, signed release on start.
    pub check_updates: bool,
    /// Set when the rules come from a file, for showing in the app.
    #[serde(skip_deserializing)]
    pub source: Option<String>,
    /// Set when the file could not be read and the strictest rules apply.
    #[serde(skip_deserializing)]
    pub problem: Option<String>,
}

impl Default for Policy {
    /// No file: everything Crane offers is allowed.
    fn default() -> Self {
        Self {
            contact: None,
            allowed_hosts: None,
            blocked_hosts: Vec::new(),
            allow_saving_secrets: true,
            allow_insecure_tls: true,
            containers: true,
            keep_history: true,
            check_updates: true,
            source: None,
            problem: None,
        }
    }
}

/// Where administrators put the policy on this system.
pub fn system_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Crane/policy.toml")
    } else if cfg!(windows) {
        let base = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("Crane").join("policy.toml")
    } else {
        PathBuf::from("/etc/crane/policy.toml")
    }
}

impl Policy {
    /// The strictest rules: what applies when a policy file is broken.
    pub fn locked(problem: String, source: &Path) -> Self {
        Self {
            contact: None,
            allowed_hosts: Some(vec!["localhost".into(), "127.0.0.1".into(), "::1".into()]),
            blocked_hosts: Vec::new(),
            allow_saving_secrets: false,
            allow_insecure_tls: false,
            containers: false,
            keep_history: false,
            check_updates: false,
            source: Some(source.display().to_string()),
            problem: Some(problem),
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => Self::locked(format!("The policy file could not be read ({e})."), path),
            Ok(text) => match toml::from_str::<Policy>(&text) {
                Ok(mut p) => {
                    p.source = Some(path.display().to_string());
                    p
                }
                Err(e) => Self::locked(format!("The policy file has a mistake: {}", e.message()), path),
            },
        }
    }

    pub fn load_system() -> Self {
        Self::load(&system_path())
    }

    /// The sentence shown when a rule says no.
    pub fn refusal(&self, what: &str) -> String {
        let who = self.contact.as_deref().map(|c| format!(" Ask {c} if you need it.")).unwrap_or_default();
        format!("{what} is turned off by your organisation's policy.{who}")
    }

    /// Whether requests may go to `host`.
    pub fn host_allowed(&self, host: &str) -> bool {
        let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
        if self.blocked_hosts.iter().any(|p| matches(p, &host)) {
            return false;
        }
        self.allowed_hosts.as_ref().is_none_or(|list| list.iter().any(|p| matches(p, &host)))
    }

    pub fn check_url(&self, url: &reqwest::Url) -> Result<(), String> {
        let host = url.host_str().unwrap_or_default();
        if self.host_allowed(host) { Ok(()) } else { Err(self.refusal(&format!("Sending to {host}"))) }
    }
}

/// `*.example.com` matches `api.example.com` and `example.com` itself.
fn matches(pattern: &str, host: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    match p.strip_prefix("*.") {
        Some(base) => host == base || host.ends_with(&format!(".{base}")),
        None => host == p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(toml: &str) -> Policy {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(&path, toml).unwrap();
        Policy::load(&path)
    }

    #[test]
    fn reaches_the_ui_in_camel_case() {
        let json = serde_json::to_value(Policy::default()).unwrap();
        for key in ["allowedHosts", "blockedHosts", "allowSavingSecrets", "allowInsecureTls", "containers", "keepHistory", "checkUpdates", "source", "problem"]
        {
            assert!(json.get(key).is_some(), "{key} missing from {json}");
        }
        assert_eq!(json["keepHistory"], true);
    }

    #[test]
    fn no_file_allows_everything() {
        let p = Policy::load(Path::new("/definitely/not/here/policy.toml"));
        assert_eq!(p, Policy::default());
        assert!(p.host_allowed("anything.dev"));
    }

    #[test]
    fn reads_the_rules() {
        let p = policy(
            r#"
contact = "it@example.com"
allowed_hosts = ["*.example.com", "localhost"]
blocked_hosts = ["admin.example.com"]
allow_saving_secrets = false
containers = false
"#,
        );
        assert!(p.host_allowed("api.example.com"));
        assert!(p.host_allowed("example.com"));
        assert!(p.host_allowed("LOCALHOST"));
        assert!(!p.host_allowed("admin.example.com"));
        assert!(!p.host_allowed("example.com.evil.dev"));
        assert!(!p.host_allowed("api.github.com"));
        assert!(!p.allow_saving_secrets && !p.containers);
        assert!(p.allow_insecure_tls && p.keep_history, "rules not in the file stay allowed");
        assert_eq!(p.refusal("Containers"), "Containers is turned off by your organisation's policy. Ask it@example.com if you need it.");
    }

    #[test]
    fn a_broken_file_locks_everything_down() {
        let p = policy("allowed_hosts = [\"x\"\ncontainers = maybe");
        assert!(p.problem.as_deref().unwrap().contains("mistake"));
        assert!(!p.containers && !p.allow_saving_secrets && !p.allow_insecure_tls && !p.keep_history && !p.check_updates);
        assert!(p.host_allowed("localhost") && !p.host_allowed("example.com"));

        let unknown = policy("allow_everything = true");
        assert!(unknown.problem.is_some(), "unknown rules are mistakes, not ignored");
    }

    #[test]
    fn checks_urls_with_ip_addresses() {
        let p = policy(r#"allowed_hosts = ["127.0.0.1", "::1"]"#);
        assert!(p.check_url(&"http://127.0.0.1:8080/x".parse().unwrap()).is_ok());
        assert!(p.check_url(&"http://[::1]:8080/x".parse().unwrap()).is_ok());
        assert!(p.check_url(&"http://10.0.0.1/".parse().unwrap()).is_err());
    }
}
