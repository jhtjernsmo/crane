//! Sends one request and measures how long the server took to answer and how
//! long the body took to arrive.

use std::time::{Duration, Instant};

use reqwest::{Client, Method};
use serde::Serialize;

use crate::{Header, RequestSpec, SendError, normalize_url};

/// Bodies bigger than this are cut when shown; `size` still has the full length.
pub const MAX_SHOWN_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    /// From sending until the status line and headers arrived.
    pub waiting_ms: u64,
    /// From the headers until the last byte of the body.
    pub download_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    /// The address actually used, after a scheme was added and redirects followed.
    pub url: String,
    pub headers: Vec<Header>,
    /// The body as text, or `None` when it is not UTF-8 (an image, a zip, ...).
    pub body: Option<String>,
    pub truncated: bool,
    pub size: usize,
    pub timing: Timing,
}

pub async fn send(client: &Client, spec: &RequestSpec, timeout: Duration) -> Result<Response, SendError> {
    let url = normalize_url(&spec.url)?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| SendError::BadUrl(spec.url.trim().to_string()))?;
    let place = place_of(&parsed);
    let method = Method::from_bytes(spec.method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|_| SendError::BadMethod(spec.method.clone()))?;

    let mut builder = client.request(method, parsed).timeout(timeout);
    for h in spec.headers.iter().filter(|h| !h.name.trim().is_empty()) {
        let name = reqwest::header::HeaderName::from_bytes(h.name.trim().as_bytes())
            .map_err(|_| SendError::BadHeader(h.name.clone()))?;
        let value = reqwest::header::HeaderValue::from_str(&h.value).map_err(|_| SendError::BadHeader(h.name.clone()))?;
        builder = builder.header(name, value);
    }
    if let Some(body) = spec.body.as_ref().filter(|b| !b.is_empty()) {
        builder = builder.body(body.clone());
    }

    let started = Instant::now();
    let res = builder.send().await.map_err(|e| explain(&e, &place, timeout))?;
    let headers_at = Instant::now();
    let status = res.status();
    let final_url = res.url().to_string();
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| Header { name: k.as_str().to_string(), value: String::from_utf8_lossy(v.as_bytes()).into_owned() })
        .collect();
    let bytes = res.bytes().await.map_err(|e| explain(&e, &place, timeout))?;
    let done = Instant::now();

    let size = bytes.len();
    let truncated = size > MAX_SHOWN_BYTES;
    let shown = &bytes[..size.min(MAX_SHOWN_BYTES)];
    let body = match std::str::from_utf8(shown) {
        Ok(text) => Some(text.to_string()),
        // A cut can land inside a character: keep the text up to it.
        Err(e) if truncated && e.error_len().is_none() => Some(String::from_utf8_lossy(&shown[..e.valid_up_to()]).into_owned()),
        Err(_) => None,
    };

    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        url: final_url,
        headers,
        body,
        truncated,
        size,
        timing: Timing {
            waiting_ms: ms(headers_at - started),
            download_ms: ms(done - headers_at),
            total_ms: ms(done - started),
        },
    })
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// "localhost:8080" or "api.github.com", for messages.
fn place_of(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

/// reqwest's errors say what failed deep down ("tcp connect error: Connection
/// refused (os error 111)"). Look through the chain and say it plainly.
fn explain(e: &reqwest::Error, place: &str, timeout: Duration) -> SendError {
    if e.is_timeout() {
        return SendError::TimedOut(place.to_string(), timeout.as_secs());
    }
    let mut chain = String::new();
    let mut source: Option<&dyn std::error::Error> = Some(e);
    while let Some(err) = source {
        chain.push_str(&err.to_string().to_ascii_lowercase());
        chain.push(' ');
        source = err.source();
    }
    if chain.contains("dns error") || chain.contains("failed to lookup") || chain.contains("name or service not known") {
        SendError::HostNotFound(place.to_string())
    } else if chain.contains("connection refused") {
        SendError::Refused(place.to_string())
    } else if chain.contains("certificate") || chain.contains("tls") || chain.contains("handshake") {
        SendError::Tls(place.to_string(), last_cause(e))
    } else {
        SendError::Other(last_cause(e))
    }
}

/// The innermost error, which is usually the one that says what happened.
fn last_cause(e: &reqwest::Error) -> String {
    let mut last: &dyn std::error::Error = e;
    while let Some(next) = last.source() {
        last = next;
    }
    last.to_string()
}
