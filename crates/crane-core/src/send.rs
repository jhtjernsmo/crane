//! Sends one request and measures how long the server took to answer and how
//! long the body took to arrive.

use std::time::{Duration, Instant};

use reqwest::Method;
use serde::Serialize;

use crate::client::timed_builder;
use std::sync::Arc;

use crate::network::Network;
use crate::policy::Policy;
use crate::vars::mask;
use crate::{Header, RequestSpec, SendError, normalize_url};

/// Redirects followed before giving up, as browsers do.
const MAX_REDIRECTS: usize = 10;

/// A redirect that was stopped, with the sentence to show for it.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

/// Bodies bigger than this are cut when shown; `size` still has the full length.
pub const MAX_SHOWN_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    /// Looking up the server's address. 0 for IP addresses.
    pub dns_ms: u64,
    /// Opening the connection, TCP and TLS together, after the lookup.
    pub connect_ms: u64,
    /// From the connection being ready until the status line and headers arrived.
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

/// How to send: the same for every request in a session.
#[derive(Debug, Clone)]
pub struct SendOptions {
    pub timeout: Duration,
    pub user_agent: String,
    /// Certificates and proxy, from the environment, variables filled in.
    pub network: Network,
    /// The organisation's rules: which hosts, and whether certificate
    /// checks may be skipped. Checked here, for the first address and for
    /// every redirect.
    pub policy: Arc<Policy>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            user_agent: concat!("Crane/", env!("CARGO_PKG_VERSION")).into(),
            network: Network::default(),
            policy: Arc::new(Policy::default()),
        }
    }
}

pub async fn send(spec: &RequestSpec, opts: &SendOptions) -> Result<Response, SendError> {
    let timeout = opts.timeout;
    if opts.network.insecure && !opts.policy.allow_insecure_tls {
        return Err(SendError::Blocked(opts.policy.refusal("Skipping certificate checks")));
    }
    let (builder, phases) = timed_builder(&opts.user_agent);
    let policy = Arc::clone(&opts.policy);
    let redirects = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error(Refused(format!("More than {MAX_REDIRECTS} redirects.")))
        } else if let Err(why) = policy.check_url(attempt.url()) {
            attempt.error(Refused(why))
        } else {
            attempt.follow()
        }
    });
    let client = opts.network.apply(builder.redirect(redirects)).await?.build().map_err(|e| SendError::Other(last_cause(&e)))?;
    let url = normalize_url(&spec.url)?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| SendError::BadUrl(spec.url.trim().to_string()))?;
    let place = place_of(&parsed);
    opts.policy.check_url(&parsed).map_err(SendError::Blocked)?;
    let method = Method::from_bytes(spec.method.trim().to_ascii_uppercase().as_bytes()).map_err(|_| SendError::BadMethod(spec.method.clone()))?;

    let mut builder = client.request(method, parsed).timeout(timeout);
    for h in spec.headers.iter().filter(|h| !h.name.trim().is_empty()) {
        let name = reqwest::header::HeaderName::from_bytes(h.name.trim().as_bytes()).map_err(|_| SendError::BadHeader(h.name.clone()))?;
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
    let headers =
        res.headers().iter().map(|(k, v)| Header { name: k.as_str().to_string(), value: String::from_utf8_lossy(v.as_bytes()).into_owned() }).collect();
    let bytes = res.bytes().await.map_err(|e| explain(&e, &place, timeout))?;
    let done = Instant::now();

    let p = phases.lock().map(|p| *p).unwrap_or_default();
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
            dns_ms: ms(p.dns),
            connect_ms: ms(p.connect_total.saturating_sub(p.dns)),
            waiting_ms: ms((headers_at - started).saturating_sub(p.connect_total)),
            download_ms: ms(done - headers_at),
            total_ms: ms(done - started),
        },
    })
}

/// Hides secret values that the server sent back, in the body, headers and URL.
pub fn mask_response(res: Response, secrets: &[(String, String)]) -> Response {
    if secrets.is_empty() {
        return res;
    }
    Response {
        url: mask(&res.url, secrets),
        headers: res.headers.into_iter().map(|h| Header { name: h.name, value: mask(&h.value, secrets) }).collect(),
        body: res.body.map(|b| mask(&b, secrets)),
        ..res
    }
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
    // A redirect we stopped says why itself.
    let mut source: Option<&dyn std::error::Error> = Some(e);
    while let Some(err) = source {
        if let Some(r) = err.downcast_ref::<Refused>() {
            return SendError::Blocked(r.0.clone());
        }
        source = err.source();
    }
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
