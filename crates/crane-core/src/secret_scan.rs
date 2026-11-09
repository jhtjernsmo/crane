//! Looks for keys, tokens and passwords in a request before it is written to
//! a file, so they do not end up in git. `{{variables}}` are fine: their
//! values live in the keyring. The UI has a copy of these rules for live
//! hints (ui/src/secretScan.ts); this one is the gate.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::http_file::SavedRequest;

static KNOWN: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\b(sk|pk|rk)_(live|test)_[A-Za-z0-9]{10,}",
        r"\bgh[pousr]_[A-Za-z0-9]{30,}",
        r"\bgithub_pat_[A-Za-z0-9_]{30,}",
        r"\bxox[abprs]-[A-Za-z0-9-]{10,}",
        r"\bAKIA[0-9A-Z]{16}\b",
        r"\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]+",
        r"\bAIza[0-9A-Za-z_-]{35}\b",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid pattern"))
    .collect()
});
static UUID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}$").expect("valid pattern"));
static SECRET_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(authorization|proxy-authorization|cookie)$|token|secret|passw(or)?d|api[-_]?key|session|credential|private").expect("valid pattern"));
/// `"password": "hunter2"` and `password=hunter2` in bodies and queries.
static NAMED_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)"?([A-Za-z0-9_-]*(?:token|secret|passw(?:or)?d|api[-_]?key|credential)[A-Za-z0-9_-]*)"?\s*[:=]\s*"?([^"&,\s{}]+)"#).expect("valid pattern")
});
static AUTH_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(bearer|basic|token|digest)\s+").expect("valid pattern"));

const MIN_RANDOM_LEN: usize = 20;
const MIN_NAMED_LEN: usize = 3;
const PREVIEW_CHARS: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Where it is, for people: "header Authorization", "address", "body".
    pub place: String,
    /// The header it is in, so the UI can replace it there.
    pub header: Option<String>,
    /// The secret itself, so the UI can move it to the keyring. It came from
    /// the UI in the same call, so nothing new is revealed.
    pub value: String,
    /// A name to suggest for the variable that replaces it.
    pub suggested_name: String,
    /// The first characters and the length, for showing without revealing.
    pub preview: String,
}

fn looks_random(word: &str) -> bool {
    if word.len() < MIN_RANDOM_LEN || UUID.is_match(word) || word.contains(['/', ':', '@', '.', '{', '}']) {
        return false;
    }
    let lower = word.chars().any(|c| c.is_ascii_lowercase());
    let upper = word.chars().any(|c| c.is_ascii_uppercase());
    let digit = word.chars().any(|c| c.is_ascii_digit());
    let kinds = [lower, upper, digit].iter().filter(|k| **k).count();
    kinds >= 2 && digit && word.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '=' | '-'))
}

/// The part of `text` that looks like a secret, if any.
pub fn secret_in(text: &str) -> Option<String> {
    for re in KNOWN.iter() {
        if let Some(m) = re.find(text) {
            return Some(m.as_str().to_string());
        }
    }
    text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '&' | '?' | '=' | ';' | '[' | ']'))
        .find(|w| looks_random(w))
        .map(str::to_string)
}

pub fn is_secret_name(name: &str) -> bool {
    SECRET_NAME.is_match(name.trim())
}

fn has_variable(text: &str) -> bool {
    !crate::vars::find_vars(text).is_empty()
}

/// Numbers, booleans and null next to a secret-sounding name are settings
/// like `"tokens_used": 12`, not secrets.
fn plausible_secret(value: &str) -> bool {
    value.len() >= MIN_NAMED_LEN
        && !value.contains("{{")
        && !value.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !matches!(value.to_ascii_lowercase().as_str(), "true" | "false" | "null")
}

fn preview(value: &str) -> String {
    let head: String = value.chars().take(PREVIEW_CHARS).collect();
    format!("{head}… ({} characters)", value.chars().count())
}

fn var_name_for(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    let n = if n == "authorization" || n == "proxy-authorization" { "token".to_string() } else { n };
    let cleaned: String = n.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let cleaned = cleaned.trim_matches('_').trim_start_matches(|c: char| c.is_ascii_digit()).to_string();
    if cleaned.is_empty() { "secret".into() } else { cleaned }
}

/// Every literal secret in the request. Empty means it is safe to write.
pub fn scan(r: &SavedRequest) -> Vec<Finding> {
    let mut found: Vec<Finding> = Vec::new();
    let mut push = |place: String, header: Option<String>, value: String, name: String| {
        if !value.is_empty() && !found.iter().any(|f| f.value == value) {
            found.push(Finding { place, header, preview: preview(&value), value, suggested_name: name });
        }
    };

    for h in r.headers.iter().filter(|h| !h.value.trim().is_empty()) {
        let credential = AUTH_SCHEME.replace(h.value.trim(), "").to_string();
        if is_secret_name(&h.name) && !has_variable(&h.value) {
            push(format!("header {}", h.name.trim()), Some(h.name.trim().to_string()), credential, var_name_for(&h.name));
        } else if let Some(s) = secret_in(&h.value) {
            push(format!("header {}", h.name.trim()), Some(h.name.trim().to_string()), s, var_name_for(&h.name));
        }
    }
    for (content, place) in [(&r.url, "address"), (&r.body, "body")] {
        for c in NAMED_VALUE.captures_iter(content) {
            if plausible_secret(&c[2]) {
                push(format!("{place} ({})", &c[1]), None, c[2].to_string(), var_name_for(&c[1]));
            }
        }
        if let Some(s) = secret_in(content) {
            push(place.to_string(), None, s, "secret".into());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_file::SavedHeader;

    fn req(url: &str, headers: &[(&str, &str)], body: &str) -> SavedRequest {
        SavedRequest {
            name: "t".into(),
            method: "POST".into(),
            url: url.into(),
            headers: headers.iter().map(|(n, v)| SavedHeader { name: (*n).into(), value: (*v).into(), enabled: true }).collect(),
            body_mode: "json".into(),
            body: body.into(),
            expect: vec![],
            save: vec![],
        }
    }

    #[test]
    fn variables_are_safe() {
        let r = req("{{base}}/me?key={{api_key}}", &[("Authorization", "Bearer {{token}}"), ("Accept", "application/json")], r#"{"password":"{{pw}}"}"#);
        assert!(scan(&r).is_empty(), "{:?}", scan(&r));
    }

    #[test]
    fn finds_tokens_in_secret_headers() {
        let f = scan(&req("x.dev", &[("Authorization", "Bearer abc123"), ("X-Api-Key", "k-1")], ""));
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].place.as_str(), f[0].value.as_str(), f[0].suggested_name.as_str()), ("header Authorization", "abc123", "token"));
        assert_eq!(f[0].header.as_deref(), Some("Authorization"));
        assert_eq!(f[1].suggested_name, "x_api_key");
        assert_eq!(f[0].preview, "abc1… (6 characters)");
    }

    #[test]
    fn finds_known_key_formats_anywhere() {
        let f = scan(&req("x.dev/charge?k=sk_live_51HxYz2eZvKYlo2C8w9a", &[("X-Trace", "ghp_1234567890abcdefghijABCDEFGHIJ123456")], ""));
        let values: Vec<&str> = f.iter().map(|x| x.value.as_str()).collect();
        assert!(values.contains(&"sk_live_51HxYz2eZvKYlo2C8w9a"), "{values:?}");
        assert!(values.contains(&"ghp_1234567890abcdefghijABCDEFGHIJ123456"));
    }

    #[test]
    fn finds_named_values_in_bodies_and_queries() {
        let f = scan(&req("x.dev/login?api_key=k3y", &[], r#"{"user": "ada", "password": "hunter2"}"#));
        let pairs: Vec<(&str, &str)> = f.iter().map(|x| (x.place.as_str(), x.value.as_str())).collect();
        assert_eq!(pairs, [("address (api_key)", "k3y"), ("body (password)", "hunter2")]);
    }

    #[test]
    fn leaves_ordinary_requests_alone() {
        let r = req(
            "https://api.example.com/v1/users/c0a8016e-6f4b-4e7a-9a3d-2f1b3c4d5e6f?page=2",
            &[("Accept", "application/json"), ("User-Agent", "Crane/0.1"), ("Authorization", "")],
            r#"{"name": "Ada Lovelace", "created": "2026-11-09T12:00:00Z", "tokens_used": 12, "password_reset": false}"#,
        );
        assert!(scan(&r).is_empty(), "{:?}", scan(&r));
    }
}
