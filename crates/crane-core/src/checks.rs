//! Checks on responses, written as comment lines in `.http` files so other
//! tools ignore them:
//!
//! ```text
//! # @expect status 200            (or 2xx)
//! # @expect time < 500ms          (or 2s)
//! # @expect header content-type contains json
//! # @expect body contains "ok"
//! # @expect json $.items length 3
//! # @expect json $.user.id exists
//! # @expect json $.user.name = "Ada"
//! # @save token = json $.access_token
//! # @save etag = header etag
//! ```
//!
//! `@save` keeps a value from the answer as a variable for the requests
//! that come after it in the same run. Saved values are treated as secrets.

use serde::Serialize;
use serde_json::Value;

use crate::Response;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// The check as written.
    pub check: String,
    pub passed: bool,
    /// What was found instead, when it failed.
    pub detail: Option<String>,
}

/// Something to read out of a response.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    Status,
    Time,
    Header(String),
    Body,
    Json(Vec<Step>),
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum Test {
    Exists,
    Equals(String),
    NotEquals(String),
    Contains(String),
    Less(f64),
    Greater(f64),
    Length(usize),
    StatusClass(u16),
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// `$.a.b[0].c`, `a.b[0]` or `$["a b"].c`.
fn parse_path(p: &str) -> Result<Vec<Step>, String> {
    let s = p.trim().strip_prefix('$').unwrap_or(p.trim());
    let mut steps = Vec::new();
    let mut chars = s.chars().peekable();
    let mut key = String::new();
    let flush = |key: &mut String, steps: &mut Vec<Step>| {
        if !key.is_empty() {
            steps.push(Step::Key(std::mem::take(key)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '.' => flush(&mut key, &mut steps),
            '[' => {
                flush(&mut key, &mut steps);
                let inner: String = chars.by_ref().take_while(|c| *c != ']').collect();
                let inner = inner.trim();
                if inner.starts_with('"') || inner.starts_with('\'') {
                    steps.push(Step::Key(unquote(inner)));
                } else {
                    steps.push(Step::Index(inner.parse().map_err(|_| format!("\"{p}\" has an index that is not a number."))?));
                }
            }
            c => key.push(c),
        }
    }
    flush(&mut key, &mut steps);
    Ok(steps)
}

fn parse_source(words: &mut std::str::SplitWhitespace<'_>, line: &str) -> Result<Source, String> {
    Ok(match words.next() {
        Some("status") => Source::Status,
        Some("time") => Source::Time,
        Some("body") => Source::Body,
        Some("header") => Source::Header(words.next().ok_or_else(|| format!("\"{line}\" needs a header name."))?.to_ascii_lowercase()),
        Some("json") => Source::Json(parse_path(words.next().ok_or_else(|| format!("\"{line}\" needs a JSON path like $.id."))?)?),
        _ => return Err(format!("\"{line}\" should start with status, time, header, body or json.")),
    })
}

fn parse_millis(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Some(n) = t.strip_suffix("ms") {
        n.trim().parse().ok()
    } else if let Some(n) = t.strip_suffix('s') {
        n.trim().parse::<f64>().ok().map(|v| v * 1000.0)
    } else {
        t.parse().ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Expect {
    source: Source,
    test: Test,
}

fn parse_expect(line: &str) -> Result<Expect, String> {
    let mut words = line.split_whitespace();
    let source = parse_source(&mut words, line)?;
    let rest: Vec<&str> = words.collect();
    let op = rest.first().copied();
    let arg = rest.get(1..).map(|r| r.join(" ")).unwrap_or_default();
    let test = match (&source, op) {
        (Source::Status, Some(code)) if rest.len() == 1 => {
            if let Some(class) = code.strip_suffix("xx").and_then(|d| d.parse::<u16>().ok()) {
                Test::StatusClass(class)
            } else {
                Test::Equals(code.to_string())
            }
        }
        (_, Some("exists")) => Test::Exists,
        (_, Some("=" | "==")) => Test::Equals(unquote(&arg)),
        (_, Some("!=")) => Test::NotEquals(unquote(&arg)),
        (_, Some("contains")) => Test::Contains(unquote(&arg)),
        (_, Some("<")) => Test::Less(parse_millis(&arg).ok_or_else(|| format!("\"{line}\" needs a number after <."))?),
        (_, Some(">")) => Test::Greater(parse_millis(&arg).ok_or_else(|| format!("\"{line}\" needs a number after >."))?),
        (_, Some("length")) => Test::Length(arg.trim().parse().map_err(|_| format!("\"{line}\" needs a whole number after length."))?),
        _ => return Err(format!("\"{line}\" is not a check Crane knows. Try: status 200, time < 500ms, json $.id exists.")),
    };
    Ok(Expect { source, test })
}

/// Lets the UI and the file reader report a typo before anything is sent.
pub fn validate(line: &str) -> Result<(), String> {
    parse_expect(line).map(|_| ())
}

pub fn validate_save(line: &str) -> Result<(), String> {
    parse_save(line).map(|_| ())
}

fn walk<'a>(v: &'a Value, steps: &[Step]) -> Option<&'a Value> {
    steps.iter().try_fold(v, |cur, step| match step {
        Step::Key(k) => cur.get(k),
        Step::Index(i) => cur.get(*i),
    })
}

/// The value found, as text (JSON strings without quotes).
fn read(source: &Source, res: &Response) -> Result<Option<Value>, String> {
    Ok(match source {
        Source::Status => Some(Value::from(res.status)),
        Source::Time => Some(Value::from(res.timing.total_ms)),
        Source::Header(name) => res.headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| Value::from(h.value.clone())),
        Source::Body => res.body.clone().map(Value::from),
        Source::Json(steps) => {
            let body = res.body.as_deref().ok_or("The answer has no text body.")?;
            let json: Value = serde_json::from_str(body).map_err(|_| "The answer is not JSON.".to_string())?;
            walk(&json, steps).cloned()
        }
    })
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn short(s: &str) -> String {
    const MAX: usize = 120;
    if s.chars().count() > MAX { format!("{}…", s.chars().take(MAX).collect::<String>()) } else { s.to_string() }
}

/// Runs one `@expect` line against the answer.
pub fn check(line: &str, res: &Response) -> CheckResult {
    let fail = |detail: String| CheckResult { check: line.to_string(), passed: false, detail: Some(detail) };
    let e = match parse_expect(line) {
        Ok(e) => e,
        Err(msg) => return fail(msg),
    };
    let found = match read(&e.source, res) {
        Ok(v) => v,
        Err(msg) => return fail(msg),
    };
    let ok = |cond: bool, detail: String| CheckResult { check: line.to_string(), passed: cond, detail: (!cond).then_some(detail) };
    let Some(v) = found else {
        return ok(matches!(e.test, Test::NotEquals(_)), "it is not there".into());
    };
    let shown = short(&text(&v));
    match &e.test {
        Test::Exists => ok(true, String::new()),
        Test::Equals(want) => {
            // `= 3` matches the number 3 and the string "3"; `= true` the boolean.
            let same = text(&v) == *want || serde_json::from_str::<Value>(want).is_ok_and(|w| w == v);
            ok(same, format!("got {shown}"))
        }
        Test::NotEquals(want) => ok(text(&v) != *want, format!("got {shown}")),
        Test::Contains(want) => {
            let has = match &v {
                Value::Array(items) => items.iter().any(|i| text(i) == *want),
                other => text(other).contains(want.as_str()),
            };
            ok(has, format!("got {shown}"))
        }
        Test::Less(n) => ok(v.as_f64().is_some_and(|x| x < *n), format!("got {shown}")),
        Test::Greater(n) => ok(v.as_f64().is_some_and(|x| x > *n), format!("got {shown}")),
        Test::Length(n) => {
            let len = match &v {
                Value::Array(a) => Some(a.len()),
                Value::Object(o) => Some(o.len()),
                Value::String(s) => Some(s.chars().count()),
                _ => None,
            };
            ok(len == Some(*n), format!("length is {}", len.map_or("not countable".into(), |l| l.to_string())))
        }
        Test::StatusClass(c) => ok(res.status / 100 == *c, format!("got {}", res.status)),
    }
}

fn parse_save(line: &str) -> Result<(String, Source), String> {
    let (name, from) = line.split_once('=').ok_or_else(|| format!("\"{line}\" should look like: token = json $.access_token"))?;
    let name = name.trim();
    if !crate::vars::is_var_name(name) {
        return Err(format!("\"{name}\" is not a usable variable name."));
    }
    let mut words = from.split_whitespace();
    let source = parse_source(&mut words, line)?;
    Ok((name.to_string(), source))
}

/// Runs one `@save` line: the variable's name and the value found.
pub fn save(line: &str, res: &Response) -> Result<(String, String), String> {
    let (name, source) = parse_save(line)?;
    let v = read(&source, res)?.ok_or_else(|| format!("{name}: the answer does not have it."))?;
    Ok((name, text(&v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Header, Timing};

    fn res() -> Response {
        Response {
            status: 201,
            status_text: "Created".into(),
            url: "http://x/".into(),
            headers: vec![Header { name: "content-type".into(), value: "application/json; charset=utf-8".into() }],
            body: Some(r#"{"id":7,"name":"Ada","tags":["a","b"],"user":{"active":true},"items":[{"sku":"c-1"},{"sku":"c-2"}],"token":"tk-123"}"#.into()),
            truncated: false,
            size: 100,
            timing: Timing { dns_ms: 0, connect_ms: 0, waiting_ms: 80, download_ms: 4, total_ms: 84 },
        }
    }

    fn passes(line: &str) -> bool {
        let r = check(line, &res());
        assert!(r.passed || r.detail.is_some(), "{line}");
        r.passed
    }

    #[test]
    fn checks_status_and_time() {
        assert!(passes("status 201"));
        assert!(passes("status 2xx"));
        assert!(!passes("status 200"));
        assert!(!passes("status 4xx"));
        assert!(passes("time < 500ms"));
        assert!(passes("time < 1s"));
        assert!(!passes("time < 50"));
        assert!(passes("time > 10"));
    }

    #[test]
    fn checks_headers_and_body() {
        assert!(passes("header Content-Type contains json"));
        assert!(passes("header content-type exists"));
        assert!(!passes("header x-missing exists"));
        assert!(passes("header x-missing != anything"));
        assert!(passes(r#"body contains "Ada""#));
    }

    #[test]
    fn checks_json() {
        assert!(passes("json $.id = 7"));
        assert!(passes("json $.id = \"7\""));
        assert!(passes("json $.name = Ada"));
        assert!(passes("json $.user.active = true"));
        assert!(passes("json $.items[1].sku = c-2"));
        assert!(passes("json items length 2"));
        assert!(passes("json $.tags contains b"));
        assert!(passes(r#"json $["name"] exists"#));
        assert!(!passes("json $.items[5] exists"));
        assert!(!passes("json $.name = Grace"));
    }

    #[test]
    fn says_why_a_check_failed() {
        assert_eq!(check("json $.name = Grace", &res()).detail.as_deref(), Some("got Ada"));
        assert_eq!(check("status 200", &res()).detail.as_deref(), Some("got 201"));
        assert_eq!(check("json $.items length 3", &res()).detail.as_deref(), Some("length is 2"));
        assert_eq!(check("json $.nope exists", &res()).detail.as_deref(), Some("it is not there"));
    }

    #[test]
    fn explains_unknown_checks() {
        assert!(validate("status").is_err());
        assert!(validate("speed < 3").unwrap_err().contains("should start with"));
        assert!(validate("json $.a[x] exists").unwrap_err().contains("index"));
        assert!(validate("time < soon").unwrap_err().contains("needs a number"));
        assert!(validate("json $.a is great").unwrap_err().contains("not a check"));
    }

    #[test]
    fn saves_values_for_later_requests() {
        assert_eq!(save("token = json $.token", &res()).unwrap(), ("token".into(), "tk-123".into()));
        assert_eq!(save("kind = header content-type", &res()).unwrap().1, "application/json; charset=utf-8");
        assert!(save("2x = json $.id", &res()).is_err());
        assert!(save("x = json $.nope", &res()).unwrap_err().contains("does not have it"));
    }
}
