use std::net::SocketAddr;

use crane_demo_api::{Config, DEMO_TOKEN, app};
use serde_json::{Value, json};

async fn start(requests_per_minute: u32) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app(Config { requests_per_minute, trust_proxy: false }).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap()
}

#[tokio::test]
async fn echoes_the_request() {
    let base = start(100).await;
    let answer: Value =
        client().post(format!("{base}/post?page=2")).header("x-demo", "yes").json(&json!({ "item": "crane-001" })).send().await.unwrap().json().await.unwrap();
    assert_eq!(answer["method"], "POST");
    assert_eq!(answer["args"]["page"], "2");
    assert_eq!(answer["headers"]["x-demo"], "yes");
    assert_eq!(answer["json"]["item"], "crane-001");
}

#[tokio::test]
async fn answers_status_redirect_and_bad_input() {
    let base = start(100).await;
    let c = client();
    assert_eq!(c.get(format!("{base}/status/418")).send().await.unwrap().status(), 418);
    assert_eq!(c.get(format!("{base}/status/99")).send().await.unwrap().status(), 400);
    let hop = c.get(format!("{base}/redirect/2")).send().await.unwrap();
    assert_eq!(hop.status(), 302);
    assert_eq!(hop.headers()["location"], "/redirect/1");
    assert_eq!(c.get(format!("{base}/delay/20000")).send().await.unwrap().status(), 400);
    assert_eq!(c.get(format!("{base}/nope")).send().await.unwrap().status(), 404);
    assert_eq!(c.get(format!("{base}/get")).send().await.unwrap().headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn orders_need_the_token_from_login() {
    let base = start(100).await;
    let c = client();
    assert_eq!(c.get(format!("{base}/orders")).send().await.unwrap().status(), 401);
    let wrong = c.post(format!("{base}/login")).json(&json!({ "username": "ada", "password": "nope" })).send().await.unwrap();
    assert_eq!(wrong.status(), 401);

    let login: Value = c.post(format!("{base}/login")).json(&json!({ "username": "ada", "password": "crane" })).send().await.unwrap().json().await.unwrap();
    let token = login["token"].as_str().unwrap();
    assert_eq!(token, DEMO_TOKEN);

    let created = c.post(format!("{base}/orders")).bearer_auth(token).json(&json!({ "item": "crane-001", "qty": 2 })).send().await.unwrap();
    assert_eq!(created.status(), 201);
    let order: Value = created.json().await.unwrap();
    let id = order["id"].as_u64().unwrap();

    let read: Value = c.get(format!("{base}/orders/{id}")).bearer_auth(token).send().await.unwrap().json().await.unwrap();
    assert_eq!(read["qty"], 2);
    let too_many = c.post(format!("{base}/orders")).bearer_auth(token).json(&json!({ "item": "x", "qty": 1000 })).send().await.unwrap();
    assert_eq!(too_many.status(), 400);

    assert_eq!(c.delete(format!("{base}/orders/{id}")).bearer_auth(token).send().await.unwrap().status(), 204);
    assert_eq!(c.get(format!("{base}/orders/{id}")).bearer_auth(token).send().await.unwrap().status(), 404);
}

#[tokio::test]
async fn limits_requests_per_address() {
    let base = start(3).await;
    let c = client();
    for _ in 0..3 {
        assert_eq!(c.get(format!("{base}/health")).send().await.unwrap().status(), 200);
    }
    let blocked = c.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(blocked.status(), 429);
    assert!(blocked.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn refuses_large_bodies() {
    let base = start(100).await;
    let big = "x".repeat(70 * 1024);
    assert_eq!(client().post(format!("{base}/post")).body(big).send().await.unwrap().status(), 413);
}
