//! Requests on disk as `.http` files, the plain-text format VS Code REST Client
//! and JetBrains IDEs also read:
//!
//! ```text
//! # @name Create order
//! # @crane body=json
//! POST {{base}}/orders
//! Accept: application/json
//! # @disabled X-Debug: 1
//! Content-Type: application/json
//!
//! {"qty": 2}
//! ```
//!
//! `# @crane` and `# @disabled` are comments to other tools, so files stay
//! usable there. One request per file; files holding several are read-only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<SavedHeader>,
    /// `none`, `json`, `text` or `form` (`form` bodies are URL-encoded text).
    pub body_mode: String,
    pub body: String,
    /// `# @expect` lines: checks on the answer (see `checks`).
    #[serde(default)]
    pub expect: Vec<String>,
    /// `# @save` lines: values kept for later requests in a run.
    #[serde(default)]
    pub save: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedHeader {
    pub name: String,
    pub value: String,
    pub enabled: bool,
}

/// A request read from a file, and whether saving over that file is safe.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadRequest {
    pub request: SavedRequest,
    /// Why Crane will not write this file back, if it will not.
    pub read_only: Option<String>,
}

const BODY_MODES: [&str; 4] = ["none", "json", "text", "form"];
const SEPARATOR: &str = "###";

fn content_type_for(mode: &str) -> Option<&'static str> {
    match mode {
        "json" => Some("application/json"),
        "text" => Some("text/plain"),
        "form" => Some("application/x-www-form-urlencoded"),
        _ => None,
    }
}

fn mode_for(content_type: &str) -> &'static str {
    let ct = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if ct == "application/json" || ct.ends_with("+json") {
        "json"
    } else if ct == "application/x-www-form-urlencoded" {
        "form"
    } else {
        "text"
    }
}

fn is_content_type(h: &SavedHeader) -> bool {
    h.name.trim().eq_ignore_ascii_case("content-type")
}

/// Writes the request. A Content-Type matching the body is added when none is
/// set, so the file also works in other tools; reading drops it again.
pub fn to_http(r: &SavedRequest) -> String {
    let mode = if BODY_MODES.contains(&r.body_mode.as_str()) { r.body_mode.as_str() } else { "text" };
    let mut out = String::new();
    if !r.name.trim().is_empty() {
        out.push_str(&format!("# @name {}\n", one_line(r.name.trim())));
    }
    if mode != "none" {
        out.push_str(&format!("# @crane body={mode}\n"));
    }
    for e in r.expect.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        out.push_str(&format!("# @expect {}\n", one_line(e)));
    }
    for v in r.save.iter().map(|v| v.trim()).filter(|v| !v.is_empty()) {
        out.push_str(&format!("# @save {}\n", one_line(v)));
    }
    out.push_str(&format!("{} {}\n", r.method.trim().to_ascii_uppercase(), one_line(r.url.trim())));
    for h in r.headers.iter().filter(|h| !h.name.trim().is_empty()) {
        let line = format!("{}: {}", one_line(h.name.trim()), one_line(&h.value));
        out.push_str(&if h.enabled { line } else { format!("# @disabled {line}") });
        out.push('\n');
    }
    let has_type = r.headers.iter().any(|h| h.enabled && is_content_type(h));
    if let (false, Some(ct)) = (has_type, content_type_for(mode)) {
        out.push_str(&format!("Content-Type: {ct}\n"));
    }
    if mode != "none" && !r.body.is_empty() {
        out.push('\n');
        out.push_str(r.body.trim_end_matches('\n'));
        out.push('\n');
    }
    out
}

/// Header values and URLs cannot hold line breaks; a stray one would turn the
/// rest into a header or body when read back.
fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

fn comment(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix('#').or_else(|| t.strip_prefix("//")).map(str::trim)
}

fn is_method(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_uppercase())
}

/// Reads the first request in a `.http` file. `fallback_name` (the file name)
/// is used when the file has no `# @name`.
pub fn from_http(text: &str, fallback_name: &str) -> Result<ReadRequest, String> {
    let text = text.replace("\r\n", "\n");
    // A `###` line starts a new request; the rest of that line is a title.
    let mut blocks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in text.lines() {
        match blocks.last_mut() {
            Some(_) if line.starts_with(SEPARATOR) => blocks.push(Vec::new()),
            Some(block) => block.push(line),
            None => {}
        }
    }
    let with_request: Vec<&Vec<&str>> = blocks.iter().filter(|b| b.iter().any(|l| !l.trim().is_empty() && comment(l).is_none())).collect();
    let Some(block) = with_request.first() else {
        return Err("There is no request in this file.".into());
    };
    let read_only = (with_request.len() > 1).then(|| {
        format!("This file holds {} requests. Crane shows the first and will not overwrite the file; save it as a new one instead.", with_request.len())
    });

    let mut lines = block.iter().copied();
    let mut name = None;
    let mut mode = None;
    let mut expect = Vec::new();
    let mut save = Vec::new();
    let request_line = loop {
        let Some(line) = lines.next() else { return Err("There is no request in this file.".into()) };
        if line.trim().is_empty() {
            continue;
        }
        match comment(line) {
            Some(c) => {
                if let Some(n) = c.strip_prefix("@name") {
                    name = Some(n.trim().to_string());
                } else if let Some(m) = c.strip_prefix("@crane body=") {
                    mode = Some(m.trim().to_string());
                } else if let Some(e) = c.strip_prefix("@expect ") {
                    expect.push(e.trim().to_string());
                } else if let Some(v) = c.strip_prefix("@save ") {
                    save.push(v.trim().to_string());
                }
            }
            None => break line.trim(),
        }
    };
    let mut words = request_line.split_whitespace();
    let first = words.next().unwrap_or_default();
    let (method, url) = if is_method(first) { (first.to_string(), words.next().unwrap_or_default()) } else { ("GET".to_string(), first) };

    let mut headers = Vec::new();
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            break;
        }
        let (enabled, text) = match comment(line) {
            Some(c) => match c.strip_prefix("@disabled") {
                Some(rest) => (false, rest.trim()),
                None => continue,
            },
            None => (true, line),
        };
        if let Some((n, v)) = text.split_once(':') {
            headers.push(SavedHeader { name: n.trim().to_string(), value: v.trim().to_string(), enabled });
        }
    }
    let body: String = lines.collect::<Vec<_>>().join("\n").trim_end_matches('\n').to_string();

    let content_type = headers.iter().find(|h| h.enabled && is_content_type(h)).map(|h| h.value.clone());
    let body_mode = match mode.filter(|m| BODY_MODES.contains(&m.as_str())) {
        Some(m) => m,
        None if body.is_empty() => "none".to_string(),
        None => mode_for(content_type.as_deref().unwrap_or("")).to_string(),
    };
    // Drop the Content-Type Crane would add by itself anyway.
    if let (Some(ct), Some(auto)) = (&content_type, content_type_for(&body_mode))
        && ct.eq_ignore_ascii_case(auto)
    {
        headers.retain(|h| !(h.enabled && is_content_type(h)));
    }

    Ok(ReadRequest {
        request: SavedRequest {
            name: name.filter(|n| !n.is_empty()).unwrap_or_else(|| fallback_name.to_string()),
            method,
            url: url.to_string(),
            headers,
            body_mode,
            body,
            expect,
            save,
        },
        read_only,
    })
}

/// What to send for a saved request: the headers that are on, the body for
/// its mode, and the Content-Type the app would add, so a run sends exactly
/// what the app sends.
pub fn to_spec(r: &SavedRequest) -> crate::RequestSpec {
    let mut headers: Vec<crate::Header> = r
        .headers
        .iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
        .map(|h| crate::Header { name: h.name.trim().to_string(), value: h.value.clone() })
        .collect();
    let has_type = headers.iter().any(|h| h.name.eq_ignore_ascii_case("content-type"));
    if let (false, Some(ct)) = (has_type, content_type_for(&r.body_mode)) {
        headers.push(crate::Header { name: "Content-Type".into(), value: ct.into() });
    }
    crate::RequestSpec { method: r.method.clone(), url: r.url.clone(), headers, body: (r.body_mode != "none").then(|| r.body.clone()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &str, value: &str, enabled: bool) -> SavedHeader {
        SavedHeader { name: name.into(), value: value.into(), enabled }
    }

    fn order() -> SavedRequest {
        SavedRequest {
            name: "Create order".into(),
            method: "POST".into(),
            url: "{{base}}/orders".into(),
            headers: vec![header("Accept", "application/json", true), header("X-Debug", "1", false)],
            body_mode: "json".into(),
            body: "{\n  \"qty\": 2\n}".into(),
            expect: vec![],
            save: vec![],
        }
    }

    #[test]
    fn writes_a_file_other_tools_can_read() {
        assert_eq!(
            to_http(&order()),
            "# @name Create order\n# @crane body=json\nPOST {{base}}/orders\nAccept: application/json\n# @disabled X-Debug: 1\nContent-Type: application/json\n\n{\n  \"qty\": 2\n}\n"
        );
    }

    #[test]
    fn reads_back_what_it_wrote() {
        let back = from_http(&to_http(&order()), "ignored").unwrap();
        assert_eq!(back.request, order());
        assert_eq!(back.read_only, None);
    }

    #[test]
    fn round_trips_every_body_mode_and_a_set_content_type() {
        for mode in BODY_MODES {
            let mut r = order();
            r.body_mode = mode.into();
            if mode == "none" {
                r.body = String::new();
            }
            assert_eq!(from_http(&to_http(&r), "x").unwrap().request, r, "{mode}");
        }
        let mut own = order();
        own.headers.push(header("Content-Type", "application/vnd.api+json", true));
        assert_eq!(from_http(&to_http(&own), "x").unwrap().request, own);
    }

    #[test]
    fn reads_files_written_by_other_tools() {
        let text = "// Get a user\nGET https://api.example.com/users/7 HTTP/1.1\r\nAuthorization: Bearer {{token}}\r\n\r\n";
        let r = from_http(text, "get-user").unwrap().request;
        assert_eq!((r.name.as_str(), r.method.as_str(), r.url.as_str()), ("get-user", "GET", "https://api.example.com/users/7"));
        assert_eq!(r.headers, [header("Authorization", "Bearer {{token}}", true)]);
        assert_eq!(r.body_mode, "none");

        let posted = from_http("POST /login\nContent-Type: application/x-www-form-urlencoded\n\nuser=ada&pass=x", "login").unwrap().request;
        assert_eq!(posted.body_mode, "form");
        assert!(posted.headers.is_empty());

        let bare = from_http("example.com/health", "health").unwrap().request;
        assert_eq!((bare.method.as_str(), bare.url.as_str()), ("GET", "example.com/health"));
    }

    #[test]
    fn keeps_checks_and_saves_as_comments() {
        let mut r = order();
        r.expect = vec!["status 201".into(), "json $.id exists".into()];
        r.save = vec!["order_id = json $.id".into()];
        let text = to_http(&r);
        assert!(
            text.starts_with("# @name Create order\n# @crane body=json\n# @expect status 201\n# @expect json $.id exists\n# @save order_id = json $.id\nPOST"),
            "{text}"
        );
        assert_eq!(from_http(&text, "x").unwrap().request, r);
    }

    #[test]
    fn will_not_overwrite_files_with_several_requests() {
        let text = "### First\nGET /a\n\n### Second\nGET /b\n";
        let read = from_http(text, "two").unwrap();
        assert_eq!(read.request.url, "/a");
        assert!(read.read_only.unwrap().contains("holds 2 requests"));
    }

    #[test]
    fn line_breaks_cannot_sneak_into_headers() {
        let mut r = order();
        r.headers = vec![header("X-Note", "one\nInjected: yes", true)];
        let back = from_http(&to_http(&r), "x").unwrap().request;
        assert_eq!(back.headers, [header("X-Note", "one Injected: yes", true)]);
    }

    #[test]
    fn empty_files_are_explained() {
        assert_eq!(from_http("# only a comment\n\n", "x").unwrap_err(), "There is no request in this file.");
    }
}
