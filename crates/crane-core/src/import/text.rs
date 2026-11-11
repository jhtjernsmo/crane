//! Small text helpers the importers share.

use std::sync::LazyLock;

use regex::Regex;

use crate::http_file::{SavedHeader, SavedRequest};

/// Characters a form value may keep as they are (RFC 3986 unreserved).
fn unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if unreserved(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// Encodes like a form, but leaves `{{variables}}` whole so Crane still
/// fills them in.
pub fn encode_keeping_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start..].find("}}") else { break };
        out.push_str(&percent_encode(&rest[..start]));
        out.push_str(&rest[start..start + len + 2]);
        rest = &rest[start + len + 2..];
    }
    out.push_str(&percent_encode(rest));
    out
}

/// `a=1&b=two%20words` from pairs.
pub fn form_body(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", encode_keeping_vars(k), encode_keeping_vars(v))).collect::<Vec<_>>().join("&")
}

/// Adds query pairs to an address that may already have some.
pub fn with_query(url: &str, pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return url.to_string();
    }
    let joiner = if url.contains('?') { '&' } else { '?' };
    format!("{url}{joiner}{}", form_body(pairs))
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub enum Basic {
    Header(String),
    /// Variables cannot be base64-encoded ahead of time.
    HasVariables,
}

pub fn basic_auth(user: &str, password: &str) -> Basic {
    if user.contains("{{") || password.contains("{{") { Basic::HasVariables } else { Basic::Header(format!("Basic {}", base64(&format!("{user}:{password}")))) }
}

pub const BASIC_WITH_VARS: &str = "Basic auth with variables cannot be written as a header ahead of time. Add an Authorization header with \"Basic \" and the base64 of user:password, as a secret";

pub fn header(name: &str, value: &str, enabled: bool) -> SavedHeader {
    SavedHeader { name: name.trim().to_string(), value: value.to_string(), enabled }
}

/// Adds a header unless the request already sets one with that name.
pub fn add_header(r: &mut SavedRequest, name: &str, value: &str) {
    if !r.headers.iter().any(|h| h.enabled && h.name.eq_ignore_ascii_case(name)) {
        r.headers.push(header(name, value, true));
    }
}

/// `:id` path segments become `{{id}}`, or the value the other tool had.
pub fn fill_path_params(url: &str, params: &[(String, String)]) -> String {
    let mut out = url.to_string();
    for (name, value) in params {
        let with = if value.is_empty() { format!("{{{{{name}}}}}") } else { value.clone() };
        for end in ["/", "?", "#"] {
            out = out.replace(&format!("/:{name}{end}"), &format!("/{with}{end}"));
        }
        if let Some(stripped) = out.strip_suffix(&format!("/:{name}")) {
            out = format!("{stripped}/{with}");
        }
    }
    out
}

static DYNAMIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*\$[A-Za-z]+\s*\}\}").expect("valid pattern"));

/// Postman's and Bruno's `{{$guid}}`, `{{$timestamp}}` and the like.
pub fn uses_made_up_values(r: &SavedRequest) -> bool {
    DYNAMIC.is_match(&r.url) || DYNAMIC.is_match(&r.body) || r.headers.iter().any(|h| DYNAMIC.is_match(&h.value))
}

pub const MADE_UP_VALUES: &str = "Uses {{$...}} values the other tool makes up (like $guid or $timestamp); Crane has none yet, so set them as variables";

/// A JavaScript-ish path like `data.items[0].id` as Crane's `$.data.items[0].id`.
pub fn json_path(js: &str) -> Option<String> {
    let js = js.trim();
    let ok = !js.is_empty() && js.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '[' | ']' | '"' | '\'' | '-' | '$'));
    ok.then(|| if js.starts_with('[') { format!("${js}") } else { format!("$.{js}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_forms_but_keeps_variables() {
        let pairs = [("user".to_string(), "ada lovelace".to_string()), ("pass".to_string(), "{{pw}}&x".to_string())];
        assert_eq!(form_body(&pairs), "user=ada%20lovelace&pass={{pw}}%26x");
        assert_eq!(with_query("https://x.dev/a?b=1", &pairs[..1]), "https://x.dev/a?b=1&user=ada%20lovelace");
        assert_eq!(encode_keeping_vars("æ"), "%C3%A6");
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(""), "");
        assert_eq!(base64("f"), "Zg==");
        assert_eq!(base64("fo"), "Zm8=");
        assert_eq!(base64("foo"), "Zm9v");
        assert_eq!(base64("ada:pw"), "YWRhOnB3");
    }

    #[test]
    fn fills_path_params() {
        let p = [("id".to_string(), String::new()), ("kind".to_string(), "books".to_string())];
        assert_eq!(fill_path_params("{{base}}/:kind/:id", &p), "{{base}}/books/{{id}}");
        assert_eq!(fill_path_params("{{base}}/:kind/:id/edit?x=1", &p), "{{base}}/books/{{id}}/edit?x=1");
    }

    #[test]
    fn turns_javascript_paths_into_json_paths() {
        assert_eq!(json_path("data.items[0].id").as_deref(), Some("$.data.items[0].id"));
        assert_eq!(json_path("token").as_deref(), Some("$.token"));
        assert_eq!(json_path("x + 1"), None);
    }
}
