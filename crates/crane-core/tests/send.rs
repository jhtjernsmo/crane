//! Sends real requests to a tiny server on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use crane_core::{Header, RequestSpec, SendError, send};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Answers one connection: reads the request, then writes `reply(request)`.
fn serve_once(reply: impl FnOnce(String) -> Vec<u8> + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap();
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        head.push_str(&String::from_utf8(body).unwrap());
        let mut stream = stream;
        stream.write_all(&reply(head)).unwrap();
    });
    format!("127.0.0.1:{}", addr.port())
}

fn http(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn spec(method: &str, url: &str) -> RequestSpec {
    RequestSpec { method: method.into(), url: url.into(), headers: vec![], body: None }
}

#[tokio::test]
async fn sends_method_headers_and_body_and_reads_the_answer() {
    let addr = serve_once(|request| http("201 Created", "text/plain", request.as_bytes()));
    let mut s = spec("post", &format!("{addr}/api/orders"));
    s.headers = vec![
        Header { name: "X-Run".into(), value: "42".into() },
        Header { name: "  ".into(), value: "empty rows are skipped".into() },
    ];
    s.body = Some(r#"{"qty":2}"#.into());

    let res = send(&reqwest::Client::new(), &s, TIMEOUT).await.unwrap();

    assert_eq!(res.status, 201);
    assert_eq!(res.status_text, "Created");
    assert!(res.url.starts_with("http://127.0.0.1:"), "a scheme was added: {}", res.url);
    let echoed = res.body.unwrap();
    assert!(echoed.starts_with("POST /api/orders HTTP/1.1\r\n"), "{echoed}");
    assert!(echoed.to_ascii_lowercase().contains("x-run: 42"));
    assert!(echoed.ends_with(r#"{"qty":2}"#));
    assert!(res.headers.iter().any(|h| h.name == "content-type" && h.value == "text/plain"));
    assert_eq!(res.size, echoed.len());
    assert!(res.timing.total_ms >= res.timing.waiting_ms);
}

#[tokio::test]
async fn binary_bodies_have_no_text() {
    let addr = serve_once(|_| http("200 OK", "image/png", &[0x89, b'P', b'N', b'G', 0xff, 0xfe]));
    let res = send(&reqwest::Client::new(), &spec("GET", &addr), TIMEOUT).await.unwrap();
    assert_eq!(res.body, None);
    assert_eq!(res.size, 6);
}

#[tokio::test]
async fn very_big_bodies_are_cut_but_measured() {
    let big = vec![b'a'; crane_core::MAX_SHOWN_BYTES + 10];
    let addr = serve_once(move |_| http("200 OK", "text/plain", &big));
    let res = send(&reqwest::Client::new(), &spec("GET", &addr), TIMEOUT).await.unwrap();
    assert!(res.truncated);
    assert_eq!(res.size, crane_core::MAX_SHOWN_BYTES + 10);
    assert_eq!(res.body.unwrap().len(), crane_core::MAX_SHOWN_BYTES);
}

#[tokio::test]
async fn says_when_nothing_is_listening() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let err = send(&reqwest::Client::new(), &spec("GET", &format!("localhost:{port}")), TIMEOUT).await.unwrap_err();
    assert_eq!(err, SendError::Refused(format!("localhost:{port}")));
    assert!(err.to_string().contains("Is the server running"));
}

#[tokio::test]
async fn says_when_the_server_is_too_slow() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let _held = listener.accept().unwrap();
        thread::sleep(Duration::from_secs(3));
    });
    let err = send(&reqwest::Client::new(), &spec("GET", &addr.to_string()), Duration::from_secs(1)).await.unwrap_err();
    assert_eq!(err, SendError::TimedOut(addr.to_string(), 1));
}

#[tokio::test]
async fn says_when_the_host_does_not_exist() {
    let err = send(&reqwest::Client::new(), &spec("GET", "https://no-such-host.invalid/"), TIMEOUT).await.unwrap_err();
    assert_eq!(err, SendError::HostNotFound("no-such-host.invalid".into()));
}

#[tokio::test]
async fn refuses_bad_methods_and_headers_before_sending() {
    let client = reqwest::Client::new();
    let err = send(&client, &spec("GE T", "localhost:1"), TIMEOUT).await.unwrap_err();
    assert_eq!(err, SendError::BadMethod("GE T".into()));
    let mut s = spec("GET", "localhost:1");
    s.headers = vec![Header { name: "Bad Name".into(), value: "x".into() }];
    assert_eq!(send(&client, &s, TIMEOUT).await.unwrap_err(), SendError::BadHeader("Bad Name".into()));
}

#[tokio::test]
async fn fills_in_variables_and_hides_secrets_the_server_echoes() {
    use crane_core::env::{Environment, MemorySecrets, SecretStore, Var, resolve};
    use crane_core::mask_response;

    let addr = serve_once(|request| {
        let mut reply = http("200 OK", "text/plain", request.as_bytes());
        // Also echo the secret in a response header.
        let token = request.lines().find_map(|l| l.strip_prefix("authorization: Bearer ")).unwrap_or("").to_string();
        let at = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        reply.splice(at..at, format!("\r\nX-Echo: {token}").into_bytes());
        reply
    });
    let store = MemorySecrets::default();
    store.set("local", "token", "tk_live_8f3k2").unwrap();
    let env = Environment {
        id: "local".into(),
        name: "Local".into(),
        production: false,
        vars: vec![
            Var { name: "base".into(), value: addr, secret: false },
            Var { name: "token".into(), value: String::new(), secret: true },
        ],
    };
    let mut s = spec("GET", "{{base}}/me?key={{token}}");
    s.headers = vec![Header { name: "Authorization".into(), value: "Bearer {{token}}".into() }];

    let resolved = resolve(&s, Some(&env), &store).unwrap();
    let res = mask_response(send(&reqwest::Client::new(), &resolved.spec, TIMEOUT).await.unwrap(), &resolved.secrets);

    let body = res.body.unwrap();
    assert!(body.starts_with("GET /me?key=‹token hidden› HTTP/1.1"), "{body}");
    assert!(body.contains("authorization: Bearer ‹token hidden›"), "{body}");
    assert!(!body.contains("tk_live"));
    assert!(!res.url.contains("tk_live"), "{}", res.url);
    let echo = res.headers.iter().find(|h| h.name == "x-echo").unwrap();
    assert_eq!(echo.value, "‹token hidden›");
}
