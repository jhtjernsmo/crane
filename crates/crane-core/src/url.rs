//! Lets people type addresses the way they say them: `localhost:8080/api`
//! and `api.github.com` work without typing the scheme.

use crate::SendError;

/// Hosts that are almost never served over HTTPS, so they default to http://.
fn is_local(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.starts_with("127.")
        || host == "[::1]"
        || host == "0.0.0.0"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
}

/// Trims the address and adds a scheme when it has none: http:// for local
/// addresses, https:// for everything else.
pub fn normalize_url(input: &str) -> Result<String, SendError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SendError::EmptyUrl);
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(SendError::BadUrl(trimmed.to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    if trimmed.contains("://") {
        return Err(SendError::BadUrl(trimmed.to_string()));
    }
    let authority = trimmed.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.rsplit_once(':') {
        Some((host, _)) if !authority.ends_with(']') => host,
        _ => authority,
    };
    let scheme = if is_local(host) { "http" } else { "https" };
    Ok(format!("{scheme}://{trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_addresses_that_have_a_scheme() {
        assert_eq!(normalize_url(" https://a.dev/x ").unwrap(), "https://a.dev/x");
        assert_eq!(normalize_url("HTTP://a.dev").unwrap(), "HTTP://a.dev");
    }

    #[test]
    fn adds_http_for_local_addresses() {
        assert_eq!(normalize_url("localhost:8080/api").unwrap(), "http://localhost:8080/api");
        assert_eq!(normalize_url("127.0.0.1:3000").unwrap(), "http://127.0.0.1:3000");
        assert_eq!(normalize_url("192.168.1.20/status").unwrap(), "http://192.168.1.20/status");
        assert_eq!(normalize_url("[::1]:8080").unwrap(), "http://[::1]:8080");
        assert_eq!(normalize_url("[::1]").unwrap(), "http://[::1]");
    }

    #[test]
    fn adds_https_for_everything_else() {
        assert_eq!(normalize_url("api.github.com/zen").unwrap(), "https://api.github.com/zen");
    }

    #[test]
    fn refuses_empty_spaced_and_other_schemes() {
        assert_eq!(normalize_url("   "), Err(SendError::EmptyUrl));
        assert!(matches!(normalize_url("a b.com"), Err(SendError::BadUrl(_))));
        assert!(matches!(normalize_url("ftp://a.com"), Err(SendError::BadUrl(_))));
    }
}
