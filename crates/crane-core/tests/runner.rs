//! A run over a small collection against a tiny API on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

use crane_core::SendOptions;
use crane_core::collection::Collection;
use crane_core::env::{Environment, MemorySecrets, Var};
use crane_core::runner::{RunOptions, request_paths, run};

const TOKEN: &str = "tk-secret-123";

/// /login gives a token, /me needs it, anything else is 404.
fn api() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let (status, reply) = if head.starts_with("POST /login") {
                ("200 OK", format!(r#"{{"token":"{TOKEN}"}}"#))
            } else if head.starts_with("GET /me") && head.contains(&format!("Bearer {TOKEN}")) {
                ("200 OK", r#"{"name":"Ada","echo":"tk-secret-123"}"#.to_string())
            } else if head.starts_with("GET /me") {
                ("401 Unauthorized", "{}".to_string())
            } else {
                ("404 Not Found", "{}".to_string())
            };
            let mut s = stream;
            let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
        }
    });
    format!("127.0.0.1:{}", addr.port())
}

fn write(dir: &std::path::Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

#[tokio::test]
async fn runs_a_collection_in_order_with_saved_values_and_checks() {
    let base = api();
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "01-login.http",
        "# @name Log in\n# @crane body=json\n# @expect status 200\n# @save token = json $.token\nPOST {{base}}/login\n\n{\"user\":\"ada\"}\n",
    );
    write(
        dir.path(),
        "02-me.http",
        "# @name Who am I\n# @expect json $.name = Ada\n# @expect json $.echo = nope\n# @expect json $.echo = {{token}}\nGET {{base}}/me\nAuthorization: Bearer {{token}}\n",
    );
    write(dir.path(), "03-missing.http", "GET {{base}}/missing\n");
    let col = Collection::open(dir.path()).unwrap();
    let paths = request_paths(&col.tree().unwrap());
    assert_eq!(paths, ["01-login.http", "02-me.http", "03-missing.http"]);

    let env = Environment {
        id: "local".into(),
        name: "Local".into(),
        production: false,
        vars: vec![Var { name: "base".into(), value: base, secret: false }],
        network: Default::default(),
    };
    let store = MemorySecrets::default();
    let mut seen = Vec::new();
    let report =
        run(&col, &paths, RunOptions { env: Some(env), store: &store, vars: vec![], send: SendOptions::default(), bail: false }, |o| seen.push(o.name.clone()))
            .await;

    assert_eq!(seen, ["Log in", "Who am I", "03-missing"]);
    let [login, me, missing] = &report.outcomes[..] else { panic!("{report:?}") };
    assert!(login.passed, "{login:?}");
    assert_eq!(login.saved, ["token"]);
    assert_eq!(me.status, Some(200), "the saved token was sent: {me:?}");
    assert!(me.checks[0].passed);
    assert!(!me.checks[1].passed);
    assert_eq!(me.checks[1].detail.as_deref(), Some("got ‹token hidden›"), "saved values are hidden in details");
    assert!(me.checks[2].passed, "checks can use saved values: {:?}", me.checks[2]);
    assert_eq!(me.checks[2].check, "json $.echo = {{token}}", "and show the line as written");
    assert!(!me.passed);
    assert_eq!((missing.status, missing.passed), (Some(404), false), "no checks: 4xx fails");
    assert_eq!((report.passed(), report.failed()), (1, 2));
    assert!(!format!("{report:?}").contains(TOKEN));
}

#[tokio::test]
async fn bails_out_at_the_first_failure() {
    let base = api();
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.http", "GET {{base}}/missing\n");
    write(dir.path(), "b.http", "POST {{base}}/login\n");
    let col = Collection::open(dir.path()).unwrap();
    let store = MemorySecrets::default();
    let report = run(
        &col,
        &request_paths(&col.tree().unwrap()),
        RunOptions { env: None, store: &store, vars: vec![("base".into(), base)], send: SendOptions::default(), bail: true },
        |_| {},
    )
    .await;
    assert_eq!(report.outcomes.len(), 1);
}

#[tokio::test]
async fn explains_missing_variables() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.http", "GET {{base}}/x\n");
    let col = Collection::open(dir.path()).unwrap();
    let store = MemorySecrets::default();
    let report = run(&col, &["a.http".into()], RunOptions { env: None, store: &store, vars: vec![], send: SendOptions::default(), bail: false }, |_| {}).await;
    assert!(report.outcomes[0].error.as_deref().unwrap().contains("{{base}}"), "{report:?}");
}
