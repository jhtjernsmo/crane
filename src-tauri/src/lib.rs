//! Tauri shell: opens the window and lets the UI send requests through Rust.
//! Requests go out from here, not from the webview, so there is no CORS and
//! local and internal addresses work like they do from curl.

use std::time::Duration;

use crane_core::{RequestSpec, Response};
use tauri::State;

/// How long a request may take before Crane gives up on it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct AppState {
    /// One client for the whole app, so connections are reused between sends.
    client: reqwest::Client,
}

/// Errors cross to the UI as the plain sentences from `SendError`.
#[tauri::command]
async fn send_request(state: State<'_, AppState>, spec: RequestSpec) -> Result<Response, String> {
    crane_core::send(&state.client, &spec, REQUEST_TIMEOUT).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Crane/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the HTTP client could not start");
    tauri::Builder::default()
        .manage(AppState { client })
        .invoke_handler(tauri::generate_handler![send_request])
        .run(tauri::generate_context!())
        .expect("error while running Crane");
}
