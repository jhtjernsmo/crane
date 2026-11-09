//! Image names like `postgres:16`, `ghcr.io/org/api:1.2` or
//! `alpine@sha256:...`, turned into where to fetch them from.

use std::fmt;

const DOCKER_HUB: &str = "docker.io";
/// Docker Hub's registry API lives on another host than its name.
const DOCKER_HUB_API: &str = "registry-1.docker.io";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// `docker.io`, `ghcr.io`, `localhost:5000`, ...
    pub registry: String,
    /// `library/postgres`, `org/api`
    pub repository: String,
    /// A tag (`16`) or a digest (`sha256:...`).
    pub reference: String,
}

impl ImageRef {
    pub fn parse(input: &str) -> Result<Self, String> {
        let s = input.trim();
        if s.is_empty() || s.contains(char::is_whitespace) || s.contains("://") {
            return Err(format!("\"{input}\" is not an image name. Try something like postgres:16 or ghcr.io/org/app:1.0."));
        }
        let (rest, digest) = match s.split_once('@') {
            Some((r, d)) if d.starts_with("sha256:") && d.len() == 71 => (r, Some(d.to_string())),
            Some(_) => return Err(format!("\"{input}\" has a digest Crane cannot read; it should be sha256: and 64 hex characters.")),
            None => (s, None),
        };
        // A first part with a dot, a colon or "localhost" is a registry host.
        let (registry, path) = match rest.split_once('/') {
            Some((first, path)) if first.contains('.') || first.contains(':') || first == "localhost" => (first.to_string(), path.to_string()),
            _ => (DOCKER_HUB.to_string(), rest.to_string()),
        };
        let (path, tag) = match path.rsplit_once(':') {
            Some((p, t)) if !t.contains('/') => (p.to_string(), Some(t.to_string())),
            _ => (path, None),
        };
        let repository = if registry == DOCKER_HUB && !path.contains('/') { format!("library/{path}") } else { path };
        let valid = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '/' | '.' | '_' | '-');
        if repository.is_empty() || !repository.chars().all(valid) || repository.contains("..") {
            return Err(format!("\"{input}\" is not an image name. Names are lowercase, like postgres or org/app."));
        }
        let host_ok = registry.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
            && !registry.starts_with(['.', '-', ':'])
            && !registry.contains("..");
        if !host_ok {
            return Err(format!("\"{input}\" has a registry name Crane cannot read."));
        }
        let reference = digest.or(tag).unwrap_or_else(|| "latest".into());
        Ok(Self { registry, repository, reference })
    }

    /// The host that answers the registry API.
    pub fn api_host(&self) -> &str {
        if self.registry == DOCKER_HUB { DOCKER_HUB_API } else { &self.registry }
    }

    /// Plain HTTP only for registries on this machine.
    pub fn scheme(&self) -> &'static str {
        let host = self.registry.split(':').next().unwrap_or_default();
        if host == "localhost" || host == "127.0.0.1" { "http" } else { "https" }
    }
}

impl fmt::Display for ImageRef {
    /// The short form people type: `postgres:16`, `ghcr.io/org/app:1.0`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repo = if self.registry == DOCKER_HUB { self.repository.strip_prefix("library/").unwrap_or(&self.repository).to_string() } else { format!("{}/{}", self.registry, self.repository) };
        let sep = if self.reference.starts_with("sha256:") { '@' } else { ':' };
        write!(f, "{repo}{sep}{}", self.reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(s: &str) -> (String, String, String) {
        let r = ImageRef::parse(s).unwrap();
        (r.registry, r.repository, r.reference)
    }

    #[test]
    fn reads_the_usual_forms() {
        assert_eq!(parts("postgres"), ("docker.io".into(), "library/postgres".into(), "latest".into()));
        assert_eq!(parts("postgres:16-alpine"), ("docker.io".into(), "library/postgres".into(), "16-alpine".into()));
        assert_eq!(parts("traefik/whoami:v1.10"), ("docker.io".into(), "traefik/whoami".into(), "v1.10".into()));
        assert_eq!(parts("ghcr.io/org/api:1.2"), ("ghcr.io".into(), "org/api".into(), "1.2".into()));
        assert_eq!(parts("localhost:5000/app"), ("localhost:5000".into(), "app".into(), "latest".into()));
        let digest = format!("sha256:{}", "a".repeat(64));
        assert_eq!(parts(&format!("alpine@{digest}")).2, digest);
    }

    #[test]
    fn prints_the_short_form_back() {
        for s in ["postgres:16", "traefik/whoami:latest", "ghcr.io/org/api:1.2"] {
            assert_eq!(ImageRef::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn knows_where_to_connect() {
        assert_eq!(ImageRef::parse("postgres").unwrap().api_host(), "registry-1.docker.io");
        assert_eq!(ImageRef::parse("localhost:5000/app").unwrap().scheme(), "http");
        assert_eq!(ImageRef::parse("ghcr.io/x/y").unwrap().scheme(), "https");
    }

    #[test]
    fn refuses_what_is_not_an_image() {
        for bad in ["", "Postgres", "https://x.io/app", "a b", "../etc", "app@md5:abc"] {
            assert!(ImageRef::parse(bad).is_err(), "{bad}");
        }
    }
}
