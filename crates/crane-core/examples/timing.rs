//! Prints the timing for one GET: `cargo run -p crane-core --example timing -- https://example.com`
use crane_core::{RequestSpec, SendOptions, send};

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| "https://example.com".into());
    let spec = RequestSpec { method: "GET".into(), url, headers: vec![], body: None };
    match send(&spec, &SendOptions::default()).await {
        Ok(res) => println!("{} {:?}", res.status, res.timing),
        Err(e) => println!("error: {e}"),
    }
}
