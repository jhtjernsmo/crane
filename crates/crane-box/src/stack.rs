//! A stack: the services a collection needs, described in `crane.toml` in
//! the collection's folder, started together in one pod.
//!
//! ```toml
//! [[service]]
//! name = "db"
//! image = "postgres:16-alpine"
//! env = { POSTGRES_PASSWORD = "{{db_password}}" }
//! ports = ["5432:5432"]
//! volumes = ["./data/db:/var/lib/postgresql/data"]
//! ready = { port = 5432 }
//!
//! [[service]]
//! name = "api"
//! image = "ghcr.io/org/api:1.4"
//! env = { DATABASE_URL = "postgres://postgres:{{db_password}}@db:5432/postgres" }
//! ports = ["8080:8080"]
//! depends_on = ["db"]
//! ready = { http = "/health", port = 8080 }
//! ```

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::valid_name;
use crate::image_ref::ImageRef;

pub const FILE: &str = "crane.toml";
const DEFAULT_READY_SECS: u64 = 60;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StackFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "service")]
    pub services: Vec<Service>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub name: String,
    pub image: String,
    /// Values may use `{{variables}}` from the active environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// `"8080:80"`: port 8080 on this computer to 80 in the stack.
    #[serde(default)]
    pub ports: Vec<String>,
    /// `"./data:/var/lib/data"` or `"./seed.sql:/seed.sql:ro"`; the left side
    /// must be inside the collection's folder.
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub ready: Option<Ready>,
}

/// When a service counts as ready, so the ones that depend on it can start.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    /// Something accepts connections on this port in the stack.
    #[serde(default)]
    pub port: Option<u16>,
    /// An HTTP GET of this path (on `port`, default 80) answers 2xx or 3xx.
    #[serde(default)]
    pub http: Option<String>,
    /// This text shows up in the service's log.
    #[serde(default)]
    pub log: Option<String>,
    /// Seconds to wait before giving up.
    #[serde(default)]
    pub timeout: Option<u64>,
}

impl Ready {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout.unwrap_or(DEFAULT_READY_SECS))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortMap {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub source: PathBuf,
    pub target: String,
    pub read_only: bool,
}

pub fn parse_port(s: &str) -> Result<PortMap, String> {
    let bad = || format!("\"{s}\" is not a port mapping. Write it as \"8080:80\" (this computer : container).");
    let (h, c) = s.trim().split_once(':').ok_or_else(bad)?;
    let num = |p: &str| p.trim().parse::<u16>().ok().filter(|n| *n > 0);
    Ok(PortMap { host: num(h).ok_or_else(bad)?, container: num(c).ok_or_else(bad)? })
}

/// A volume whose source stays inside `folder`, so a crane.toml from a
/// cloned repository cannot hand your other files to a container.
pub fn parse_volume(s: &str, folder: &Path) -> Result<Volume, String> {
    let parts: Vec<&str> = s.split(':').collect();
    let (src, target, read_only) = match parts.as_slice() {
        [src, target] => (*src, *target, false),
        [src, target, "ro"] => (*src, *target, true),
        [src, target, "rw"] => (*src, *target, false),
        _ => return Err(format!("\"{s}\" is not a volume. Write it as \"./data:/var/lib/data\", with :ro at the end for read-only.")),
    };
    let rel = Path::new(src);
    if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("The volume \"{s}\" must be inside the collection's folder, like ./data. Crane will not share other folders with a container."));
    }
    if !target.starts_with('/') || target.contains("/../") || target.ends_with("/..") {
        return Err(format!("In \"{s}\", the container side must be an absolute path like /var/lib/data."));
    }
    Ok(Volume { source: folder.join(rel), target: target.to_string(), read_only })
}

impl StackFile {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("crane.toml could not be read: {}", e.message()))
    }

    /// The pod's name: the `name` field, or the folder's.
    pub fn pod_name(&self, folder: &Path) -> String {
        let raw = self.name.clone().unwrap_or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "stack".into()));
        let slug: String = raw.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
        let slug = slug.trim_matches('-').to_string();
        if valid_name(&slug) { slug } else { "stack".into() }
    }

    /// Checks everything that can be checked before starting anything.
    pub fn validate(&self, folder: &Path) -> Result<(), String> {
        if self.services.is_empty() {
            return Err("crane.toml has no [[service]] yet.".into());
        }
        let mut names = HashSet::new();
        let mut host_ports = HashSet::new();
        for s in &self.services {
            if !valid_name(&s.name) {
                return Err(format!("\"{}\" is not a usable service name: use lowercase letters, digits, - and _.", s.name));
            }
            if !names.insert(s.name.as_str()) {
                return Err(format!("Two services are called {}.", s.name));
            }
            ImageRef::parse(&s.image).map_err(|e| format!("{}: {e}", s.name))?;
            for p in &s.ports {
                let m = parse_port(p).map_err(|e| format!("{}: {e}", s.name))?;
                if !host_ports.insert(m.host) {
                    return Err(format!("Port {} on this computer is used twice in crane.toml.", m.host));
                }
            }
            for v in &s.volumes {
                parse_volume(v, folder).map_err(|e| format!("{}: {e}", s.name))?;
            }
            if let Some(r) = &s.ready
                && r.port.is_none()
                && r.http.is_none()
                && r.log.is_none()
            {
                return Err(format!("{}: ready needs a port, http or log check.", s.name));
            }
        }
        for s in &self.services {
            for d in &s.depends_on {
                if !names.contains(d.as_str()) {
                    return Err(format!("{} depends on {d}, which is not in crane.toml.", s.name));
                }
            }
        }
        self.start_order().map(|_| ())
    }

    /// Services in an order where each comes after what it depends on.
    pub fn start_order(&self) -> Result<Vec<&Service>, String> {
        let mut done: Vec<&Service> = Vec::new();
        let mut left: Vec<&Service> = self.services.iter().collect();
        while !left.is_empty() {
            let (ready, wait): (Vec<&Service>, Vec<&Service>) =
                left.into_iter().partition(|s| s.depends_on.iter().all(|d| done.iter().any(|x| &x.name == d)));
            if ready.is_empty() {
                let names: Vec<&str> = wait.iter().map(|s| s.name.as_str()).collect();
                return Err(format!("These services wait for each other in a circle: {}.", names.join(", ")));
            }
            done.extend(ready);
            left = wait;
        }
        Ok(done)
    }
}

/// A commented starting point for a new crane.toml.
pub const TEMPLATE: &str = r#"# Services this collection needs. Start them from Crane's Containers tab.
# They share one network, so they reach each other by name (db:5432).
# Values in env can use {{variables}} from the active environment;
# secrets come from the system keyring and never land in this file.

[[service]]
name = "db"
image = "postgres:16-alpine"
env = { POSTGRES_PASSWORD = "{{db_password}}" }
ports = ["5432:5432"]
ready = { port = 5432 }

# [[service]]
# name = "api"
# image = "ghcr.io/your-org/api:latest"
# env = { DATABASE_URL = "postgres://postgres:{{db_password}}@db:5432/postgres" }
# ports = ["8080:8080"]
# depends_on = ["db"]
# ready = { http = "/health", port = 8080 }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        PathBuf::from("/home/me/api-tests")
    }

    const SHOP: &str = r#"
name = "Shop API"

[[service]]
name = "api"
image = "ghcr.io/org/api:1.4"
ports = ["8080:8080"]
depends_on = ["db", "cache"]
ready = { http = "/health", port = 8080 }

[[service]]
name = "db"
image = "postgres:16-alpine"
env = { POSTGRES_PASSWORD = "{{db_password}}" }
ports = ["5432:5432"]
volumes = ["./data/db:/var/lib/postgresql/data", "./seed.sql:/docker-entrypoint-initdb.d/seed.sql:ro"]
ready = { port = 5432, timeout = 90 }

[[service]]
name = "cache"
image = "redis:7"
"#;

    #[test]
    fn reads_and_orders_a_stack() {
        let s = StackFile::parse(SHOP).unwrap();
        s.validate(&folder()).unwrap();
        let order: Vec<&str> = s.start_order().unwrap().iter().map(|x| x.name.as_str()).collect();
        assert_eq!(order, ["db", "cache", "api"]);
        assert_eq!(s.pod_name(&folder()), "shop-api");
        assert_eq!(s.services[1].ready.as_ref().unwrap().timeout(), Duration::from_secs(90));
        assert_eq!(s.services[1].env["POSTGRES_PASSWORD"], "{{db_password}}");
    }

    #[test]
    fn the_template_is_a_valid_stack() {
        let s = StackFile::parse(TEMPLATE).unwrap();
        s.validate(&folder()).unwrap();
        assert_eq!(s.pod_name(&folder()), "api-tests");
    }

    #[test]
    fn volumes_stay_in_the_folder() {
        let v = parse_volume("./seed.sql:/seed.sql:ro", &folder()).unwrap();
        assert_eq!((v.source, v.target.as_str(), v.read_only), (folder().join("./seed.sql"), "/seed.sql", true));
        for bad in ["/home/me/.ssh:/keys", "../secrets:/s", "./data/../../x:/x", "./data:relative", "./a:/b:rx", "~/x:/x:ro:ro"] {
            assert!(parse_volume(bad, &folder()).is_err(), "{bad}");
        }
    }

    #[test]
    fn explains_mistakes() {
        let check = |toml: &str| StackFile::parse(toml).and_then(|s| s.validate(&folder())).unwrap_err();
        assert!(check("").contains("no [[service]]"));
        assert!(check("[[service]]\nname = \"db\"\nimage = \"pg\"\nport = 1").contains("could not be read"));
        assert!(check("[[service]]\nname = \"Db\"\nimage = \"pg\"").contains("not a usable service name"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\ndepends_on = [\"b\"]").contains("which is not in crane.toml"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\nports = [\"80\"]").contains("not a port mapping"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\nports = [\"80:80\"]\n[[service]]\nname = \"b\"\nimage = \"y\"\nports = [\"80:81\"]").contains("used twice"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\ndepends_on = [\"b\"]\n[[service]]\nname = \"b\"\nimage = \"y\"\ndepends_on = [\"a\"]").contains("in a circle"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\nready = { timeout = 5 }").contains("needs a port, http or log"));
        assert!(check("[[service]]\nname = \"a\"\nimage = \"x\"\nvolumes = [\"/etc:/etc\"]").contains("inside the collection's folder"));
    }
}

// ---- Running a stack ----------------------------------------------------------

pub mod run {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{PortMap, Ready, StackFile, parse_port, parse_volume};
    use crate::engine::{ContainerOptions, Engine, Forward, Pod, forward};
    use crate::error::{BoxError, Result};
    use crate::proto::Mount;

    const POLL: Duration = Duration::from_millis(250);
    /// A service without a ready check counts as up if it is still running after this.
    const SETTLE: Duration = Duration::from_millis(500);
    const LOG_LINES_ON_FAILURE: usize = 12;
    const HTTP_DEFAULT_PORT: u16 = 80;
    /// Pull progress is reported in steps of this many percent.
    const PULL_REPORT_STEP: u64 = 10;

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct StackEvent {
        pub service: String,
        pub step: String,
    }

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RunningService {
        pub name: String,
        pub image: String,
        pub ports: Vec<PortMap>,
    }

    pub struct RunningStack {
        pub name: String,
        pub pod: Arc<Pod>,
        pub services: Vec<RunningService>,
        _forwards: Vec<Forward>,
    }

    /// Starts every service in order, waiting for each to be ready.
    /// `fill` turns `{{variables}}` in env values into their values. On
    /// failure everything started so far is stopped again.
    pub async fn up(
        engine: &Engine,
        stack: &StackFile,
        folder: &std::path::Path,
        fill: &(dyn Fn(&str) -> std::result::Result<String, String> + Sync),
        progress: &(dyn Fn(StackEvent) + Sync),
    ) -> Result<RunningStack> {
        stack.validate(folder).map_err(BoxError::BadInput)?;
        let name = stack.pod_name(folder);
        let say = |service: &str, step: String| progress(StackEvent { service: service.into(), step });
        let pod = Arc::new(engine.start_pod(&name).await?);
        let mut services = Vec::new();
        let mut forwards = Vec::new();
        let result: Result<()> = async {
            let all: Vec<String> = stack.services.iter().map(|s| s.name.clone()).collect();
            for s in stack.start_order().map_err(BoxError::BadInput)? {
                let image = match engine.image(&s.image) {
                    Some(i) => i,
                    None => {
                        say(&s.name, format!("Pulling {}", s.image));
                        // Only every tenth: a pull reports each chunk it gets.
                        let last = std::sync::atomic::AtomicU64::new(0);
                        engine
                            .pull(&s.image, |p| {
                                let Some(pct) = (p.done * 100).checked_div(p.total) else { return };
                                let step = pct / PULL_REPORT_STEP * PULL_REPORT_STEP;
                                if step > last.swap(step, std::sync::atomic::Ordering::Relaxed) {
                                    say(&s.name, format!("Pulling {} {step}%", s.image));
                                }
                            })
                            .await?
                    }
                };
                let mut env = Vec::new();
                for (k, v) in &s.env {
                    let value = fill(v).map_err(|e| BoxError::BadInput(format!("{}: {e}", s.name)))?;
                    env.push((k.clone(), value));
                }
                let mut mounts = Vec::new();
                for v in &s.volumes {
                    let vol = parse_volume(v, folder).map_err(BoxError::BadInput)?;
                    // A missing source becomes a folder, like other tools do.
                    if !vol.source.exists() {
                        std::fs::create_dir_all(&vol.source)?;
                    }
                    let real = vol.source.canonicalize()?;
                    let root = folder.canonicalize()?;
                    if !real.starts_with(&root) {
                        return Err(BoxError::BadInput(format!("{}: the volume {v} leads outside the collection's folder.", s.name)));
                    }
                    mounts.push(Mount { source: real.display().to_string(), target: vol.target, read_only: vol.read_only });
                }
                let opts = ContainerOptions {
                    args: s.command.clone(),
                    env,
                    mounts,
                    user: s.user.clone(),
                    hosts: all.iter().filter(|n| **n != s.name).cloned().collect(),
                };
                say(&s.name, "Starting".into());
                let spec = engine.prepare(&name, &s.name, &image, &opts).await?;
                pod.run(spec).await?;
                say(&s.name, "Waiting until it is ready".into());
                wait_ready(engine, &pod, &name, &s.name, s.ready.as_ref()).await?;
                let mut ports = Vec::new();
                for p in &s.ports {
                    let m = parse_port(p).map_err(BoxError::BadInput)?;
                    forwards.push(forward(Arc::clone(&pod), m.host, m.container).await?);
                    ports.push(m);
                }
                services.push(RunningService { name: s.name.clone(), image: image.name, ports });
                say(&s.name, "Ready".into());
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            forwards.clear();
            let _ = pod.shutdown().await;
            let _ = engine.clean_pod(&name).await;
            return Err(e);
        }
        Ok(RunningStack { name, pod, services, _forwards: forwards })
    }

    /// Stops every service and removes their files (logs are kept).
    pub async fn down(engine: &Engine, stack: RunningStack) -> Result<()> {
        let name = stack.name.clone();
        stack.pod.shutdown().await?;
        drop(stack);
        engine.clean_pod(&name).await
    }

    fn last_lines(log: &str) -> String {
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(LOG_LINES_ON_FAILURE)..].join("\n")
    }

    async fn wait_ready(engine: &Engine, pod: &Pod, pod_name: &str, service: &str, ready: Option<&Ready>) -> Result<()> {
        let started = Instant::now();
        let timeout = ready.map(Ready::timeout).unwrap_or(SETTLE);
        loop {
            let state = pod.status().await?.into_iter().find(|c| c.name == service);
            if let Some(c) = state.filter(|c| !c.running) {
                let log = last_lines(&engine.log_tail(pod_name, service, 16 * 1024));
                return Err(BoxError::Runtime(format!(
                    "{service} stopped (exit code {}) before it was ready. Its last lines:\n{log}",
                    c.exit_code.map_or("?".into(), |c| c.to_string())
                )));
            }
            let ok = match ready {
                None => started.elapsed() >= SETTLE,
                Some(r) => {
                    if let Some(path) = &r.http {
                        http_ok(pod, r.port.unwrap_or(HTTP_DEFAULT_PORT), path).await
                    } else if let Some(port) = r.port {
                        pod.connect(port).await.is_ok()
                    } else if let Some(text) = &r.log {
                        engine.log_tail(pod_name, service, 256 * 1024).contains(text.as_str())
                    } else {
                        true
                    }
                }
            };
            if ok {
                return Ok(());
            }
            if started.elapsed() > timeout {
                let log = last_lines(&engine.log_tail(pod_name, service, 16 * 1024));
                return Err(BoxError::Runtime(format!(
                    "{service} was not ready after {} seconds. Its last lines:\n{log}",
                    timeout.as_secs()
                )));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// A plain HTTP/1.1 GET inside the pod; true for 2xx and 3xx.
    async fn http_ok(pod: &Pod, port: u16, path: &str) -> bool {
        let Ok(mut stream) = pod.connect(port).await else { return false };
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nUser-Agent: Crane\r\nConnection: close\r\n\r\n");
        if stream.write_all(req.as_bytes()).await.is_err() {
            return false;
        }
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]);
        head.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()).is_some_and(|c| (200..400).contains(&c))
    }
}
