//! A tiny shop behind a login, for trying tokens, `@save` and Run all.
//! Everyone shares one store; it holds at most `MAX_ORDERS` and is wiped
//! every hour.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{DEMO_PASSWORD, DEMO_TOKEN, Shared, bearer, error};

const MAX_ORDERS: usize = 500;
const LIST_LIMIT: usize = 50;
const MAX_ITEM_LEN: usize = 64;
const MAX_QTY: u32 = 100;
const MAX_USERNAME_LEN: usize = 40;
const WIPE_EVERY: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    id: u64,
    item: String,
    qty: u32,
    created_at: u64,
}

struct Store {
    next_id: u64,
    orders: BTreeMap<u64, Order>,
    wiped_at: Instant,
}

pub struct Orders(Mutex<Store>);

impl Default for Orders {
    fn default() -> Self {
        Self(Mutex::new(Store { next_id: 1, orders: BTreeMap::new(), wiped_at: Instant::now() }))
    }
}

impl Orders {
    fn with<T>(&self, f: impl FnOnce(&mut Store) -> T) -> T {
        let mut store = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if store.wiped_at.elapsed() >= WIPE_EVERY {
            store.orders.clear();
            store.wiped_at = Instant::now();
        }
        f(&mut store)
    }
}

#[derive(Deserialize)]
pub struct Login {
    username: String,
    password: String,
}

pub async fn login(Json(login): Json<Login>) -> Response {
    let username = login.username.trim();
    if username.is_empty() || username.chars().count() > MAX_USERNAME_LEN {
        return error(StatusCode::BAD_REQUEST, "username must be 1 to 40 characters");
    }
    if login.password != DEMO_PASSWORD {
        return error(StatusCode::UNAUTHORIZED, "wrong password (it is \"crane\")");
    }
    Json(json!({ "token": DEMO_TOKEN, "username": username })).into_response()
}

/// The answer to send instead when the request lacks the demo token.
fn refused(headers: &HeaderMap) -> Option<Response> {
    match bearer(headers) {
        Some(DEMO_TOKEN) => None,
        Some(_) => Some(error(StatusCode::UNAUTHORIZED, "that token is not valid, get one from POST /login")),
        None => {
            let mut response = error(StatusCode::UNAUTHORIZED, "send Authorization: Bearer <token>, get one from POST /login");
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            Some(response)
        }
    }
}

pub async fn me(headers: HeaderMap) -> Response {
    refused(&headers).unwrap_or_else(|| Json(json!({ "loggedIn": true })).into_response())
}

pub async fn list(State(state): Shared, headers: HeaderMap) -> Response {
    if let Some(response) = refused(&headers) {
        return response;
    }
    let orders: Vec<Order> = state.orders.with(|s| s.orders.values().rev().take(LIST_LIMIT).cloned().collect());
    Json(json!({ "orders": orders })).into_response()
}

#[derive(Deserialize)]
pub struct NewOrder {
    item: String,
    qty: u32,
}

pub async fn create(State(state): Shared, headers: HeaderMap, Json(new): Json<NewOrder>) -> Response {
    if let Some(response) = refused(&headers) {
        return response;
    }
    let item = new.item.trim();
    if item.is_empty() || item.chars().count() > MAX_ITEM_LEN {
        return error(StatusCode::BAD_REQUEST, "item must be 1 to 64 characters");
    }
    if !(1..=MAX_QTY).contains(&new.qty) {
        return error(StatusCode::BAD_REQUEST, "qty must be from 1 to 100");
    }
    let created_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let order = state.orders.with(|s| {
        if s.orders.len() >= MAX_ORDERS {
            s.orders.pop_first();
        }
        let order = Order { id: s.next_id, item: item.to_string(), qty: new.qty, created_at };
        s.next_id += 1;
        s.orders.insert(order.id, order.clone());
        order
    });
    (StatusCode::CREATED, Json(order)).into_response()
}

fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "no order with that id (orders are wiped every hour)")
}

pub async fn get_one(State(state): Shared, headers: HeaderMap, Path(id): Path<u64>) -> Response {
    if let Some(response) = refused(&headers) {
        return response;
    }
    match state.orders.with(|s| s.orders.get(&id).cloned()) {
        Some(order) => Json(order).into_response(),
        None => not_found(),
    }
}

pub async fn delete(State(state): Shared, headers: HeaderMap, Path(id): Path<u64>) -> Response {
    if let Some(response) = refused(&headers) {
        return response;
    }
    match state.orders.with(|s| s.orders.remove(&id)) {
        Some(_) => StatusCode::NO_CONTENT.into_response(),
        None => not_found(),
    }
}
