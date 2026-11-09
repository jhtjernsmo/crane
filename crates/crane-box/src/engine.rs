//! Crane's side of the engine: pulling and storing images, starting pod
//! helpers and talking to them, and forwarding ports to this computer.
//!
//! Everything lives in one folder (`base`):
//! `blobs/sha256/` downloads, `images/*.json` pulled images,
//! `rootfs/<id>/` unpacked images, `pods/<pod>/<container>/` each
//! container's own changes, and `logs/<pod>/<container>.log`.

use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nix::sys::socket::{AddressFamily, SockFlag, SockType, setsockopt, socketpair, sockopt};
use nix::sys::time::TimeVal;
use serde::Serialize;

use crate::error::{BoxError, Result};
use crate::helper::{CONTROL_FD, HELPER_ARG};
use crate::image::{ImageConfig, ImageRecord};
use crate::image_ref::ImageRef;
use crate::proto::wire::{recv, send};
use crate::proto::{ContainerState, Mount, Reply, Request, RunSpec};
use crate::registry::Registry;

/// How long Crane waits for a pod to answer before calling it stuck.
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

/// How to start the helper: this program again, or a separate one in tests.
#[derive(Debug, Clone)]
pub struct Helper {
    program: PathBuf,
}

impl Helper {
    pub fn this_program() -> Result<Self> {
        Ok(Self { program: std::env::current_exe()? })
    }

    pub fn program(path: impl Into<PathBuf>) -> Self {
        Self { program: path.into() }
    }

    fn command(&self, sub: &str) -> std::process::Command {
        let mut c = std::process::Command::new(&self.program);
        c.arg(HELPER_ARG).arg(sub);
        c
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub step: String,
    pub done: u64,
    pub total: u64,
}

/// What to run, on top of what the image says.
#[derive(Debug, Clone, Default)]
pub struct ContainerOptions {
    /// Replaces the image's command (not its entrypoint).
    pub args: Option<Vec<String>>,
    pub env: Vec<(String, String)>,
    pub mounts: Vec<Mount>,
    pub user: Option<String>,
    /// Other names in the same pod, for /etc/hosts.
    pub hosts: Vec<String>,
}

pub struct Engine {
    base: PathBuf,
    helper: Helper,
}

pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= 63
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

fn check_name(what: &str, name: &str) -> Result<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(BoxError::BadInput(format!("\"{name}\" is not a usable {what} name: use lowercase letters, digits, - and _.")))
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Engine {
    pub fn new(base: impl Into<PathBuf>, helper: Helper) -> Self {
        Self { base: base.into(), helper }
    }

    fn blobs(&self) -> PathBuf {
        self.base.join("blobs/sha256")
    }
    fn records(&self) -> PathBuf {
        self.base.join("images")
    }
    fn rootfs(&self, config_digest: &str) -> PathBuf {
        self.base.join("rootfs").join(config_digest.trim_start_matches("sha256:"))
    }
    fn pod_dir(&self, pod: &str) -> PathBuf {
        self.base.join("pods").join(pod)
    }
    pub fn log_path(&self, pod: &str, container: &str) -> PathBuf {
        self.base.join("logs").join(pod).join(format!("{container}.log"))
    }

    // ---- Images -----------------------------------------------------------

    pub async fn pull(&self, name: &str, progress: impl Fn(PullProgress) + Send + Sync) -> Result<ImageRecord> {
        let image = ImageRef::parse(name).map_err(BoxError::BadInput)?;
        let step = |s: &str, done: u64, total: u64| progress(PullProgress { step: s.into(), done, total });
        step("Looking up the image", 0, 0);
        let mut reg = Registry::new(image.clone())?;
        let manifest = reg.manifest().await?;
        let total = manifest.config.size + manifest.layers.iter().map(|l| l.size).sum::<u64>();
        let config_path = reg.blob(&manifest.config, &self.blobs(), |_| {}).await?;
        let config = ImageConfig::from_blob(&tokio::fs::read(&config_path).await?)?;
        let mut before = manifest.config.size;
        let mut layers = Vec::new();
        for l in &manifest.layers {
            let path = reg.blob(l, &self.blobs(), |n| step("Downloading", before + n.min(l.size), total)).await?;
            before += l.size;
            layers.push(path);
        }
        step("Unpacking", total, total);
        let rootfs = self.rootfs(&manifest.config.digest);
        let done_marker = rootfs.with_extension("done");
        if !done_marker.exists() {
            if rootfs.exists() {
                self.remove_owned(std::slice::from_ref(&rootfs)).await?;
            }
            self.unpack(&rootfs, &layers).await?;
            tokio::fs::write(&done_marker, b"").await?;
        }
        let record = ImageRecord {
            name: image.to_string(),
            manifest_digest: manifest.digest.clone(),
            config_digest: manifest.config.digest.clone(),
            layers: manifest.layers.iter().map(|l| l.digest.clone()).collect(),
            size: total,
            pulled_at: now_secs(),
            config,
        };
        self.save_record(&record).await?;
        Ok(record)
    }

    async fn save_record(&self, r: &ImageRecord) -> Result<()> {
        tokio::fs::create_dir_all(self.records()).await?;
        let path = self.records().join(format!("{}.json", record_file(&r.name)));
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, serde_json::to_vec_pretty(r).map_err(std::io::Error::other)?).await?;
        tokio::fs::rename(tmp, path).await?;
        Ok(())
    }

    pub fn images(&self) -> Vec<ImageRecord> {
        let Ok(dir) = std::fs::read_dir(self.records()) else { return Vec::new() };
        let mut out: Vec<ImageRecord> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn image(&self, name: &str) -> Option<ImageRecord> {
        let wanted = ImageRef::parse(name).ok()?.to_string();
        self.images().into_iter().find(|r| r.name == wanted)
    }

    /// Forgets an image and frees what no other image uses.
    pub async fn remove_image(&self, name: &str) -> Result<()> {
        let record = self.image(name).ok_or_else(|| BoxError::BadInput(format!("{name} is not pulled.")))?;
        tokio::fs::remove_file(self.records().join(format!("{}.json", record_file(&record.name)))).await?;
        let rest = self.images();
        if !rest.iter().any(|r| r.config_digest == record.config_digest) {
            let rootfs = self.rootfs(&record.config_digest);
            let _ = tokio::fs::remove_file(rootfs.with_extension("done")).await;
            if rootfs.exists() {
                self.remove_owned(&[rootfs]).await?;
            }
        }
        for digest in std::iter::once(&record.config_digest).chain(&record.layers) {
            let used = rest.iter().any(|r| &r.config_digest == digest || r.layers.contains(digest));
            if !used {
                let _ = tokio::fs::remove_file(self.blobs().join(digest.trim_start_matches("sha256:"))).await;
            }
        }
        Ok(())
    }

    // ---- Helper calls -----------------------------------------------------

    async fn helper_call(&self, sub: &str, args: Vec<String>) -> Result<serde_json::Value> {
        let mut cmd = tokio::process::Command::from(self.helper.command(sub));
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let out = cmd.output().await?;
        let text = String::from_utf8_lossy(&out.stdout);
        let value: serde_json::Value = serde_json::from_str(text.lines().last().unwrap_or("{}")).unwrap_or(serde_json::Value::Null);
        if let Some(e) = value.get("error").and_then(|e| e.as_str()) {
            return Err(BoxError::Runtime(e.to_string()));
        }
        if !out.status.success() {
            return Err(BoxError::Runtime(format!("The container helper failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(value)
    }

    async fn unpack(&self, dest: &Path, layers: &[PathBuf]) -> Result<()> {
        let mut args = vec![dest.display().to_string()];
        args.extend(layers.iter().map(|p| p.display().to_string()));
        self.helper_call("unpack", args).await.map(|_| ())
    }

    /// Removes folders whose files belong to ids inside the namespace.
    async fn remove_owned(&self, paths: &[PathBuf]) -> Result<()> {
        tokio::fs::create_dir_all(&self.base).await?;
        let mut args = vec![self.base.display().to_string()];
        args.extend(paths.iter().map(|p| p.display().to_string()));
        self.helper_call("remove", args).await.map(|_| ())
    }

    // ---- Pods ---------------------------------------------------------------

    /// Starts an empty pod: a user and network namespace to put containers in.
    pub async fn start_pod(&self, name: &str) -> Result<Pod> {
        check_name("pod", name)?;
        let (ours, theirs) = socketpair(AddressFamily::Unix, SockType::SeqPacket, None, SockFlag::SOCK_CLOEXEC).map_err(|e| BoxError::Runtime(e.to_string()))?;
        let timeout = TimeVal::new(REPLY_TIMEOUT.as_secs() as i64, 0);
        setsockopt(&ours, sockopt::ReceiveTimeout, &timeout).map_err(|e| BoxError::Runtime(e.to_string()))?;
        let mut cmd = self.helper.command("pod");
        let fd = theirs.as_raw_fd();
        // SAFETY: dup2 is async-signal-safe; it puts the socket on the fd the
        // helper expects and clears close-on-exec for it.
        unsafe {
            cmd.pre_exec(move || {
                if nix::libc::dup2(fd, CONTROL_FD) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit());
        let child = cmd.spawn()?;
        drop(theirs);
        let mut pod = Pod { name: name.to_string(), sock: Arc::new(Mutex::new(ours)), child: Mutex::new(Some(child)), full_mapping: false };
        match pod.wait_reply().await? {
            (Reply::Ready { full_mapping }, _) => {
                pod.full_mapping = full_mapping;
                Ok(pod)
            }
            (Reply::Error { message }, _) => Err(BoxError::Namespaces(message)),
            (other, _) => Err(BoxError::Runtime(format!("The pod answered oddly: {other:?}"))),
        }
    }

    /// Makes fresh folders for a container and the spec to run it with.
    pub async fn prepare(&self, pod: &str, name: &str, image: &ImageRecord, opts: &ContainerOptions) -> Result<RunSpec> {
        check_name("container", name)?;
        let dir = self.pod_dir(pod).join(name);
        if dir.exists() {
            self.remove_owned(std::slice::from_ref(&dir)).await?;
        }
        for sub in ["upper", "work", "merged"] {
            tokio::fs::create_dir_all(dir.join(sub)).await?;
        }
        let log = self.log_path(pod, name);
        tokio::fs::create_dir_all(log.parent().unwrap_or(&self.base)).await?;
        tokio::fs::write(&log, b"").await?;
        let mut env = image.config.env.clone();
        env.extend(opts.env.iter().map(|(k, v)| format!("{k}={v}")));
        let s = |p: PathBuf| p.display().to_string();
        Ok(RunSpec {
            name: name.to_string(),
            rootfs: s(self.rootfs(&image.config_digest)),
            upper: s(dir.join("upper")),
            work: s(dir.join("work")),
            merged: s(dir.join("merged")),
            log: s(log),
            args: image.config.command(opts.args.as_deref()),
            env,
            cwd: image.config.working_dir.clone(),
            user: opts.user.clone().unwrap_or_else(|| image.config.user.clone()),
            mounts: opts.mounts.clone(),
            hosts: opts.hosts.clone(),
        })
    }

    /// Removes a stopped pod's container folders (logs are kept).
    pub async fn clean_pod(&self, pod: &str) -> Result<()> {
        let dir = self.pod_dir(pod);
        if dir.exists() { self.remove_owned(&[dir]).await } else { Ok(()) }
    }

    /// The end of a container's log, at most `max` bytes.
    pub fn log_tail(&self, pod: &str, container: &str, max: u64) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut f) = std::fs::File::open(self.log_path(pod, container)) else { return String::new() };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = f.seek(SeekFrom::Start(len.saturating_sub(max)));
        let mut buf = Vec::new();
        let _ = f.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    }
}

/// `postgres:16` becomes `postgres_16`, safe as a file name.
fn record_file(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// A running pod helper and the line to it.
pub struct Pod {
    pub name: String,
    sock: Arc<Mutex<OwnedFd>>,
    child: Mutex<Option<Child>>,
    /// Whether ids other than root work inside (via /etc/subuid).
    pub full_mapping: bool,
}

impl Pod {
    async fn wait_reply(&self) -> Result<(Reply, Option<OwnedFd>)> {
        let sock = Arc::clone(&self.sock);
        tokio::task::spawn_blocking(move || {
            let s = sock.lock().map_err(|_| BoxError::Runtime("pod lock".into()))?;
            recv::<Reply>(s.as_fd())?.ok_or_else(|| BoxError::Runtime("The pod stopped answering.".into()))
        })
        .await
        .map_err(|e| BoxError::Runtime(e.to_string()))?
    }

    async fn request(&self, req: Request) -> Result<(Reply, Option<OwnedFd>)> {
        let sock = Arc::clone(&self.sock);
        tokio::task::spawn_blocking(move || {
            let s = sock.lock().map_err(|_| BoxError::Runtime("pod lock".into()))?;
            send(s.as_fd(), &req, None)?;
            match recv::<Reply>(s.as_fd())? {
                Some((Reply::Error { message }, _)) => Err(BoxError::Runtime(message)),
                Some(r) => Ok(r),
                None => Err(BoxError::Runtime("The pod stopped answering.".into())),
            }
        })
        .await
        .map_err(|e| BoxError::Runtime(e.to_string()))?
    }

    /// Starts a container; returns its pid as the pod sees it.
    pub async fn run(&self, spec: RunSpec) -> Result<i32> {
        match self.request(Request::Run(Box::new(spec))).await? {
            (Reply::Started { pid }, _) => Ok(pid),
            (other, _) => Err(BoxError::Runtime(format!("Unexpected answer: {other:?}"))),
        }
    }

    /// Asks the container to stop, and kills it after `grace`.
    pub async fn stop(&self, name: &str, grace: Duration) -> Result<()> {
        self.request(Request::Stop { name: name.into(), grace_secs: grace.as_secs() }).await.map(|_| ())
    }

    pub async fn status(&self) -> Result<Vec<ContainerState>> {
        match self.request(Request::Status).await? {
            (Reply::Status { containers }, _) => Ok(containers),
            (other, _) => Err(BoxError::Runtime(format!("Unexpected answer: {other:?}"))),
        }
    }

    /// A TCP connection to `port` inside the pod's network.
    pub async fn connect(&self, port: u16) -> Result<tokio::net::TcpStream> {
        match self.request(Request::Connect { port }).await? {
            (Reply::Connected, Some(fd)) => {
                let std = std::net::TcpStream::from(fd);
                std.set_nonblocking(true)?;
                Ok(tokio::net::TcpStream::from_std(std)?)
            }
            (other, _) => Err(BoxError::Runtime(format!("Unexpected answer: {other:?}"))),
        }
    }

    /// The pod helper's process id on this computer.
    pub fn helper_pid(&self) -> Option<u32> {
        self.child.lock().ok()?.as_ref().map(Child::id)
    }

    /// Stops every container and the pod itself.
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.request(Request::Shutdown).await;
        let child = self.child.lock().ok().and_then(|mut c| c.take());
        if let Some(mut c) = child {
            tokio::task::spawn_blocking(move || c.wait()).await.map_err(|e| BoxError::Runtime(e.to_string()))??;
        }
        Ok(())
    }
}

impl Drop for Pod {
    fn drop(&mut self) {
        // The helper stops everything when its socket closes; just reap it.
        if let Ok(mut c) = self.child.lock()
            && let Some(mut child) = c.take()
        {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// Listens on 127.0.0.1:`host_port` and passes each connection to
/// `container_port` in the pod. Dropping it stops listening.
pub struct Forward {
    pub host_port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn forward(pod: Arc<Pod>, host_port: u16, container_port: u16) -> Result<Forward> {
    // Only this computer: a test stack is not for the rest of the network.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", host_port)).await.map_err(|e| {
        BoxError::Runtime(format!("Port {host_port} on this computer is taken or not allowed ({e}). Pick another in the ports list."))
    })?;
    let host_port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        while let Ok((mut outside, _)) = listener.accept().await {
            let pod = Arc::clone(&pod);
            tokio::spawn(async move {
                if let Ok(mut inside) = pod.connect(container_port).await {
                    let _ = tokio::io::copy_bidirectional(&mut outside, &mut inside).await;
                }
            });
        }
    });
    Ok(Forward { host_port, task })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_simple() {
        for ok in ["db", "api-1", "my_app.v2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Db", "-x", "a/b", "..", &"x".repeat(64)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn record_files_are_safe() {
        assert_eq!(record_file("ghcr.io/org/app:1.2"), "ghcr.io_org_app_1.2");
    }
}
