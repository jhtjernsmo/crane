//! Runs the demo API. Settings come from the environment:
//! `BIND` (default 127.0.0.1:8080), `REQUESTS_PER_MINUTE` (default 120)
//! and `TRUST_PROXY=1` when a proxy is the only way in.

use std::net::SocketAddr;

use crane_demo_api::{Config, app};

const DEFAULT_BIND: &str = "127.0.0.1:8080";
const DEFAULT_REQUESTS_PER_MINUTE: u32 = 120;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let bind = std::env::var("BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let requests_per_minute = match std::env::var("REQUESTS_PER_MINUTE") {
        Ok(v) => v.parse().map_err(|_| std::io::Error::other("REQUESTS_PER_MINUTE must be a number"))?,
        Err(_) => DEFAULT_REQUESTS_PER_MINUTE,
    };
    let trust_proxy = std::env::var("TRUST_PROXY").is_ok_and(|v| v == "1");

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("crane-demo-api listening on {bind}");
    let app = app(Config { requests_per_minute, trust_proxy }).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await
}

/// Stops on Ctrl+C, and on SIGTERM from systemd where there is one.
async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut term) = signal(SignalKind::terminate()) else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
