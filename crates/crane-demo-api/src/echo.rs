//! Endpoints that answer with what was asked for: the request itself, a
//! status code, a delay or a chain of redirects.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::error;

const MAX_DELAY_MS: u64 = 10_000;
const MAX_REDIRECTS: u32 = 10;

/// Headers the proxy in front adds; echoing them would only confuse.
const PROXY_HEADERS: [&str; 4] = ["x-forwarded-for", "x-forwarded-proto", "x-forwarded-host", "via"];

pub async fn echo(method: Method, uri: Uri, Query(args): Query<BTreeMap<String, String>>, headers: HeaderMap, body: Bytes) -> Json<Value> {
    let headers: BTreeMap<&str, String> = headers
        .iter()
        .filter(|(name, _)| !PROXY_HEADERS.contains(&name.as_str()))
        .map(|(name, value)| (name.as_str(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
        .collect();
    let parsed: Option<Value> = serde_json::from_slice(&body).ok();
    let data = if parsed.is_some() { String::new() } else { String::from_utf8_lossy(&body).into_owned() };
    Json(json!({
        "method": method.as_str(),
        "path": uri.path(),
        "args": args,
        "headers": headers,
        "json": parsed,
        "data": data,
    }))
}

pub async fn sample_json() -> Json<Value> {
    Json(json!({
        "id": 42,
        "name": "Crane",
        "tags": ["api", "client", "open source"],
        "owner": { "login": "demo", "verified": true },
        "stats": { "requests": 1280, "checksPassed": 1274 }
    }))
}

pub async fn status(Path(code): Path<String>) -> Response {
    match code.parse::<u16>().ok().filter(|c| (200..=599).contains(c)).and_then(|c| StatusCode::from_u16(c).ok()) {
        Some(status) => (status, Json(json!({ "status": status.as_u16() }))).into_response(),
        None => error(StatusCode::BAD_REQUEST, "status must be a number from 200 to 599"),
    }
}

pub async fn delay(Path(ms): Path<String>) -> Response {
    let Some(ms) = ms.parse::<u64>().ok().filter(|ms| *ms <= MAX_DELAY_MS) else {
        return error(StatusCode::BAD_REQUEST, "delay must be a number of milliseconds from 0 to 10000");
    };
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Json(json!({ "delayedMs": ms })).into_response()
}

pub async fn redirect(Path(n): Path<String>) -> Response {
    let Some(n) = n.parse::<u32>().ok().filter(|n| (1..=MAX_REDIRECTS).contains(n)) else {
        return error(StatusCode::BAD_REQUEST, "redirect count must be from 1 to 10");
    };
    let next = if n == 1 { "/get".to_string() } else { format!("/redirect/{}", n - 1) };
    (StatusCode::FOUND, [(header::LOCATION, next)]).into_response()
}
