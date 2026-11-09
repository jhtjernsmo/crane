//! `{{name}}` variables: finding them, filling them in, and hiding secret
//! values again when a server echoes them back.

/// Allowed in a variable name: letters, digits, `_`, `-` and `.`.
pub fn is_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

enum Piece<'a> {
    Text(&'a str),
    Var(&'a str),
}

/// Splits text into literal parts and variable names. `{{ a }}` and `{{a}}`
/// are the same; braces around anything that is not a name stay as text.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else { break };
        let name = after[..close].trim();
        if is_var_name(name) {
            out.push(Piece::Text(&rest[..open]));
            out.push(Piece::Var(name));
        } else {
            out.push(Piece::Text(&rest[..open + 2 + close + 2]));
        }
        rest = &after[close + 2..];
    }
    out.push(Piece::Text(rest));
    out
}

/// The variable names used in `text`, in order, without repeats.
pub fn find_vars(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for p in pieces(text) {
        if let Piece::Var(n) = p
            && !names.iter().any(|x| x == n)
        {
            names.push(n.to_string());
        }
    }
    names
}

/// Fills in every variable `lookup` knows. Unknown names are collected into
/// the error, so the user hears about all of them at once.
pub fn substitute(text: &str, lookup: &impl Fn(&str) -> Option<String>) -> Result<String, Vec<String>> {
    let mut out = String::with_capacity(text.len());
    let mut missing: Vec<String> = Vec::new();
    for p in pieces(text) {
        match p {
            Piece::Text(t) => out.push_str(t),
            Piece::Var(n) => match lookup(n) {
                Some(v) => out.push_str(&v),
                None if !missing.iter().any(|m| m == n) => missing.push(n.to_string()),
                None => {}
            },
        }
    }
    if missing.is_empty() { Ok(out) } else { Err(missing) }
}

/// Values shorter than this are not hidden: replacing every "1" or "ok" in a
/// response would wreck it and hide nothing worth hiding.
pub const MIN_MASKED_LEN: usize = 4;

/// What a hidden secret looks like in a response.
pub fn mask_label(name: &str) -> String {
    format!("‹{name} hidden›")
}

/// Replaces each secret value in `text`, also in its URL-encoded form.
pub fn mask(text: &str, secrets: &[(String, String)]) -> String {
    let mut out = text.to_string();
    let mut sorted: Vec<&(String, String)> = secrets.iter().filter(|(_, v)| v.len() >= MIN_MASKED_LEN).collect();
    // Longest first, so a secret that contains another is hidden whole.
    sorted.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (name, value) in sorted {
        let label = mask_label(name);
        out = out.replace(value.as_str(), &label);
        let encoded = url_encode(value);
        if encoded != *value {
            out = out.replace(&encoded, &label);
        }
    }
    out
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        move |n: &str| owned.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone())
    }

    #[test]
    fn names_follow_simple_rules() {
        for ok in ["base", "api_key", "v2.token", "_x", "my-var"] {
            assert!(is_var_name(ok), "{ok}");
        }
        for bad in ["", "2fa", "a b", "a}", "æ"] {
            assert!(!is_var_name(bad), "{bad}");
        }
    }

    #[test]
    fn finds_each_name_once() {
        assert_eq!(find_vars("{{base}}/users/{{ id }}?t={{base}}"), ["base", "id"]);
        assert!(find_vars("{ not } {{ also not! }} {{").is_empty());
    }

    #[test]
    fn fills_in_known_names() {
        let got = substitute("{{base}}/users/{{ id }}", &vars(&[("base", "http://localhost:8080"), ("id", "7")]));
        assert_eq!(got.unwrap(), "http://localhost:8080/users/7");
    }

    #[test]
    fn leaves_text_that_only_looks_like_a_variable() {
        let got = substitute(r#"{"a": {{"b": 1}}} {{ not a name }}"#, &vars(&[]));
        assert_eq!(got.unwrap(), r#"{"a": {{"b": 1}}} {{ not a name }}"#);
    }

    #[test]
    fn lists_every_unknown_name() {
        let got = substitute("{{a}} {{b}} {{a}} {{known}}", &vars(&[("known", "x")]));
        assert_eq!(got.unwrap_err(), ["a", "b"]);
    }

    #[test]
    fn values_are_not_searched_for_more_variables() {
        let got = substitute("{{a}}", &vars(&[("a", "{{b}}")]));
        assert_eq!(got.unwrap(), "{{b}}");
    }

    #[test]
    fn masks_secrets_plain_and_url_encoded() {
        let secrets = vec![("token".to_string(), "s3cr3t/+=".to_string())];
        let text = r#"{"auth":"Bearer s3cr3t/+=","url":"https://x.dev/?t=s3cr3t%2F%2B%3D"}"#;
        assert_eq!(mask(text, &secrets), r#"{"auth":"Bearer ‹token hidden›","url":"https://x.dev/?t=‹token hidden›"}"#);
    }

    #[test]
    fn hides_the_longer_secret_first_and_skips_tiny_ones() {
        let secrets = vec![
            ("short".to_string(), "abcd".to_string()),
            ("long".to_string(), "abcdefgh".to_string()),
            ("tiny".to_string(), "ok".to_string()),
        ];
        assert_eq!(mask("abcdefgh abcd ok", &secrets), "‹long hidden› ‹short hidden› ok");
    }
}
