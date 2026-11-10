//! A small public API to try Crane against: echo, status codes, delays,
//! redirects, a login that hands out a token, and an orders store that
//! needs it.
//!
//! It never logs requests and keeps nothing on disk. People will send real
//! keys to a demo server by mistake, so nothing they send outlives the
//! answer, except orders, which are capped and wiped every hour.

mod echo;
mod limit;
mod orders;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use serde_json::json;

pub use limit::RateLimit;
pub use orders::Orders;

/// The token `/login` hands out. It is public on purpose: it only opens the
/// demo's own orders store.
pub const DEMO_TOKEN: &str = "crane-demo-token";
pub const DEMO_PASSWORD: &str = "crane";
const MAX_BODY: usize = 64 * 1024;

pub struct Config {
    pub requests_per_minute: u32,
    /// Take the client address from `X-Forwarded-For`. Only for when a
    /// proxy is the sole way in, as in `deploy/demo-api`.
    pub trust_proxy: bool,
}

pub struct AppState {
    pub config: Config,
    pub limit: RateLimit,
    pub orders: Orders,
}

pub type Shared = State<Arc<AppState>>;

pub fn app(config: Config) -> Router {
    let state = Arc::new(AppState { limit: RateLimit::new(config.requests_per_minute), orders: Orders::default(), config });
    Router::new()
        .route("/", get(index))
        .route("/health", get(|| async { "ok" }))
        .route("/get", get(echo::echo))
        .route("/post", post(echo::echo))
        .route("/anything", any(echo::echo))
        .route("/anything/{*rest}", any(echo::echo))
        .route("/json", get(echo::sample_json))
        .route("/status/{code}", any(echo::status))
        .route("/delay/{ms}", any(echo::delay))
        .route("/redirect/{n}", get(echo::redirect))
        .route("/login", post(orders::login))
        .route("/me", get(orders::me))
        .route("/orders", get(orders::list).post(orders::create))
        .route("/orders/{id}", get(orders::get_one).delete(orders::delete))
        .fallback(|| async { error(StatusCode::NOT_FOUND, "no such endpoint, see / for the list") })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(Arc::clone(&state), rate_limit))
        .layer(middleware::from_fn(no_store))
        .with_state(state)
}

pub fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim)
}

async fn index() -> Json<serde_json::Value> {
    Json(json!({
        "name": "Crane demo API",
        "docs": "https://github.com/jhtjernsmo/crane/tree/main/examples/demo",
        "note": "Nothing you send is logged or stored, except orders, which are wiped every hour.",
        "endpoints": {
            "GET /get, POST /post, ANY /anything": "echo the request back",
            "GET /json": "a sample JSON document",
            "ANY /status/{code}": "answer with that status (200-599)",
            "ANY /delay/{ms}": "answer after that many milliseconds (at most 10000)",
            "GET /redirect/{n}": "redirect n times (at most 10), then land on /get",
            "POST /login": "send {\"username\": \"...\", \"password\": \"crane\"} to get a token",
            "GET /me": "who is logged in (needs the token)",
            "GET, POST /orders": "list or create orders (needs the token)",
            "GET, DELETE /orders/{id}": "read or delete one order (needs the token)"
        }
    }))
}

async fn rate_limit(State(state): Shared, ConnectInfo(peer): ConnectInfo<SocketAddr>, request: Request, next: Next) -> Response {
    let ip = limit::client_ip(peer, request.headers(), state.config.trust_proxy);
    match state.limit.check(ip) {
        Ok(()) => next.run(request).await,
        Err(retry_after) => {
            let mut response = error(StatusCode::TOO_MANY_REQUESTS, "too many requests, slow down");
            response.headers_mut().insert(header::RETRY_AFTER, retry_after.into());
            response
        }
    }
}

/// Answers may echo secrets back, so ask caches not to keep them.
async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
