//! Pulls real images and runs them. Needs the internet and Linux user
//! namespaces, so it only runs when asked:
//! `cargo test -p crane-box --test live -- --ignored --test-threads=1`

use std::sync::Arc;
use std::time::Duration;

use crane_box::engine::{ContainerOptions, Engine, Helper, forward};

const HELPER: &str = env!("CARGO_BIN_EXE_crane-box-helper");

fn engine(dir: &tempfile::TempDir) -> Engine {
    Engine::new(dir.path().join("box"), Helper::program(HELPER))
}

async fn wait_for_exit(pod: &crane_box::engine::Pod, name: &str) -> Option<i32> {
    for _ in 0..100 {
        let s = pod.status().await.unwrap();
        if let Some(c) = s.iter().find(|c| c.name == name && !c.running) {
            return c.exit_code;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[tokio::test]
#[ignore]
async fn runs_a_command_in_alpine() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir);
    let image = e.pull("alpine:3.20", |_| {}).await.unwrap();
    assert_eq!(image.name, "alpine:3.20");

    let pod = e.start_pod("test").await.unwrap();
    assert!(pod.full_mapping, "this machine has /etc/subuid, so other users should work");
    let script = "echo hello from $(hostname); id -u; ls /root >/dev/null && echo root-home; cat /etc/os-release | head -1; \
                  wget -q -T 2 -O- http://example.com >/dev/null 2>&1 && echo online || echo offline; \
                  echo pid1=$(cat /proc/1/comm)";
    let opts = ContainerOptions { args: Some(vec!["sh".into(), "-c".into(), script.into()]), ..Default::default() };
    let spec = e.prepare("test", "box1", &image, &opts).await.unwrap();
    pod.run(spec).await.unwrap();
    assert_eq!(wait_for_exit(&pod, "box1").await, Some(0));
    let log = e.log_tail("test", "box1", 4096);
    assert!(log.contains("hello from box1"), "{log}");
    assert!(log.contains("\n0\n"), "runs as root inside: {log}");
    assert!(log.contains("Alpine Linux"), "{log}");
    assert!(log.contains("offline"), "no internet inside: {log}");

    // A user other than root works thanks to the subordinate ids.
    let opts = ContainerOptions { args: Some(vec!["sh".into(), "-c".into(), "id -u; touch /tmp/x && echo can-write-tmp".into()]), user: Some("nobody".into()), ..Default::default() };
    let spec = e.prepare("test", "box2", &image, &opts).await.unwrap();
    pod.run(spec).await.unwrap();
    assert_eq!(wait_for_exit(&pod, "box2").await, Some(0));
    let log = e.log_tail("test", "box2", 4096);
    assert!(log.contains("65534"), "{log}");
    assert!(log.contains("can-write-tmp"), "{log}");

    pod.shutdown().await.unwrap();
    e.clean_pod("test").await.unwrap();
    e.remove_image("alpine:3.20").await.unwrap();
    assert!(e.images().is_empty());
}

#[tokio::test]
#[ignore]
async fn serves_http_through_a_forwarded_port() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir);
    let image = e.pull("traefik/whoami:v1.10", |_| {}).await.unwrap();
    let pod = Arc::new(e.start_pod("web").await.unwrap());
    let spec = e.prepare("web", "whoami", &image, &ContainerOptions::default()).await.unwrap();
    pod.run(spec).await.unwrap();
    let fwd = forward(Arc::clone(&pod), 0, 80).await.unwrap();

    let url = format!("http://127.0.0.1:{}/", fwd.host_port);
    let mut body = String::new();
    for _ in 0..50 {
        if let Ok(res) = reqwest::get(&url).await {
            body = res.text().await.unwrap_or_default();
            if !body.is_empty() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(body.contains("Hostname: whoami"), "{body}");

    pod.stop("whoami", Duration::from_secs(5)).await.unwrap();
    assert!(wait_for_exit(&pod, "whoami").await.is_some());
    drop(fwd);
    pod.shutdown().await.unwrap();
    e.clean_pod("web").await.unwrap();
}

/// Processes of ours whose command line is exactly `args`.
fn processes(args: &[&str]) -> usize {
    let want: Vec<u8> = args.iter().flat_map(|a| a.bytes().chain(std::iter::once(0))).collect();
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter(|e| std::fs::read(e.path().join("cmdline")).is_ok_and(|c| c == want))
        .count()
}

#[tokio::test]
#[ignore]
async fn containers_end_when_the_pod_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir);
    let image = e.pull("alpine:3.20", |_| {}).await.unwrap();
    let pod = e.start_pod("life").await.unwrap();
    let opts = ContainerOptions { args: Some(vec!["sleep".into(), "4242".into()]), ..Default::default() };
    pod.run(e.prepare("life", "sleeper", &image, &opts).await.unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(processes(&["sleep", "4242"]), 1);

    let pid = pod.helper_pid().unwrap();
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL).unwrap();
    let mut gone = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if processes(&["sleep", "4242"]) == 0 {
            gone = true;
            break;
        }
    }
    assert!(gone, "the container outlived its pod");
    drop(pod);
    e.clean_pod("life").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn runs_postgres_which_switches_user() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(&dir);
    let image = e.pull("postgres:16-alpine", |_| {}).await.unwrap();
    let pod = e.start_pod("pg").await.unwrap();
    let opts = ContainerOptions { env: vec![("POSTGRES_PASSWORD".into(), "test-only".into())], ..Default::default() };
    pod.run(e.prepare("pg", "db", &image, &opts).await.unwrap()).await.unwrap();
    let mut ready = false;
    for _ in 0..300 {
        let log = e.log_tail("pg", "db", 64 * 1024);
        if log.matches("database system is ready to accept connections").count() >= 2 {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let log = e.log_tail("pg", "db", 64 * 1024);
    assert!(ready, "{log}");
    let stream = pod.connect(5432).await;
    assert!(stream.is_ok(), "port 5432 answers inside the pod");
    pod.shutdown().await.unwrap();
    e.clean_pod("pg").await.unwrap();
}
