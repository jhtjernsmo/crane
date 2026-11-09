//! The pod helper: one process per pod, living in its own user and network
//! namespace. Every container in the pod is its child, with its own mount,
//! PID, UTS and IPC namespaces, and they share the pod's network, so they
//! reach each other on localhost. The network has only a loopback device:
//! no internet unless Crane adds it later.
//!
//! Runs single-threaded in a process of its own (see `helper`), because
//! forking and switching namespaces is not safe in a threaded program.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, OpenHow, ResolveFlag, open, openat2};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sched::{CloneFlags, unshare};
use nix::sys::signal::{SigSet, SigmaskHow, Signal, kill, sigprocmask};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use nix::sys::stat::Mode;
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Gid, Pid, Uid, chdir, execve, fork, pipe2, pivot_root, setgid, setgroups, sethostname, setuid};

use crate::proto::wire::{recv, send};
use crate::proto::{ContainerState, Reply, Request, RunSpec};
use crate::userns::{self, Mapping};

const TICK: Duration = Duration::from_millis(200);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Device files every container gets, bound from the host.
const DEVICES: [&str; 6] = ["null", "zero", "full", "random", "urandom", "tty"];
/// Signals pid 1 passes on to the program.
const FORWARDED: [Signal; 6] = [Signal::SIGTERM, Signal::SIGINT, Signal::SIGQUIT, Signal::SIGHUP, Signal::SIGUSR1, Signal::SIGUSR2];

struct Running {
    pid: Pid,
    running: bool,
    exit_code: Option<i32>,
    kill_at: Option<Instant>,
}

/// What every container start needs from the pod.
struct Ctx {
    mapping: Mapping,
    /// Only the pod holds the write end. When the pod dies, however it dies,
    /// each container's pid 1 sees the pipe close and ends the container.
    life_r: OwnedFd,
    life_w: OwnedFd,
}

/// Entry point of the pod helper. `control` is the socket to Crane.
pub fn run(control: OwnedFd) -> ! {
    let code = match setup().and_then(|mapping| {
        let (life_r, life_w) = pipe2(OFlag::O_CLOEXEC).map_err(|e| e.desc().to_string())?;
        Ok(Ctx { mapping, life_r, life_w })
    }) {
        Ok(ctx) => {
            let _ = send(control.as_fd(), &Reply::Ready { full_mapping: ctx.mapping == Mapping::Full }, None);
            serve(&control, &ctx)
        }
        Err(e) => {
            let _ = send(control.as_fd(), &Reply::Error { message: e }, None);
            1
        }
    };
    std::process::exit(code)
}

fn setup() -> Result<Mapping, String> {
    let mapping = userns::enter().map_err(|e| e.to_string())?;
    unshare(CloneFlags::CLONE_NEWNET).map_err(|e| format!("Could not make the pod's network: {}", e.desc()))?;
    loopback_up().map_err(|e| format!("Could not start the pod's loopback network: {e}"))?;
    // Containers' pid 1 becomes our child when its parent exits, so we can wait for it.
    nix::sys::prctl::set_child_subreaper(true).map_err(|e| e.desc().to_string())?;
    Ok(mapping)
}

/// `ip link set lo up`, without needing the ip tool.
fn loopback_up() -> std::io::Result<()> {
    use nix::libc;
    // SAFETY: plain ioctl calls on a socket we own, with a zeroed ifreq whose
    // name we fill in; no pointers escape.
    unsafe {
        let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if sock < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut req: libc::ifreq = std::mem::zeroed();
        for (i, b) in b"lo\0".iter().enumerate() {
            req.ifr_name[i] = *b as libc::c_char;
        }
        let ok = libc::ioctl(sock, libc::SIOCGIFFLAGS as _, &mut req) == 0 && {
            req.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
            libc::ioctl(sock, libc::SIOCSIFFLAGS as _, &req) == 0
        };
        let err = std::io::Error::last_os_error();
        libc::close(sock);
        if ok { Ok(()) } else { Err(err) }
    }
}

fn serve(control: &OwnedFd, ctx: &Ctx) -> i32 {
    let mut boxes: HashMap<String, Running> = HashMap::new();
    loop {
        let mut fds = [PollFd::new(control.as_fd(), PollFlags::POLLIN)];
        let ready = poll(&mut fds, PollTimeout::try_from(TICK).unwrap_or(PollTimeout::NONE)).unwrap_or(0);
        if ready > 0 {
            match recv::<Request>(control.as_fd()) {
                Ok(Some((req, _))) => {
                    if handle(control, req, &mut boxes, ctx) {
                        stop_all(&mut boxes);
                        return 0;
                    }
                }
                // Crane went away: take everything down with us.
                Ok(None) | Err(_) => {
                    stop_all(&mut boxes);
                    return 0;
                }
            }
        }
        reap(&mut boxes);
        let now = Instant::now();
        for b in boxes.values_mut().filter(|b| b.running && b.kill_at.is_some_and(|t| now >= t)) {
            let _ = kill(b.pid, Signal::SIGKILL);
            b.kill_at = None;
        }
    }
}

/// Handles one request; `true` means shut down.
fn handle(control: &OwnedFd, req: Request, boxes: &mut HashMap<String, Running>, ctx: &Ctx) -> bool {
    let reply = |r: Reply, fd: Option<i32>| {
        let _ = send(control.as_fd(), &r, fd);
    };
    match req {
        Request::Run(spec) => {
            if boxes.get(&spec.name).is_some_and(|b| b.running) {
                reply(Reply::Error { message: format!("{} is already running.", spec.name) }, None);
                return false;
            }
            match start(&spec, ctx) {
                Ok(pid) => {
                    boxes.insert(spec.name.clone(), Running { pid, running: true, exit_code: None, kill_at: None });
                    reply(Reply::Started { pid: pid.as_raw() }, None);
                }
                Err(message) => reply(Reply::Error { message }, None),
            }
        }
        Request::Stop { name, grace_secs } => match boxes.get_mut(&name) {
            Some(b) if b.running => {
                let _ = kill(b.pid, Signal::SIGTERM);
                b.kill_at = Some(Instant::now() + Duration::from_secs(grace_secs));
                reply(Reply::Ok, None);
            }
            Some(_) => reply(Reply::Ok, None),
            None => reply(Reply::Error { message: format!("There is no container called {name}.") }, None),
        },
        Request::Connect { port } => {
            let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(stream) => reply(Reply::Connected, Some(stream.as_raw_fd())),
                Err(e) => reply(Reply::Error { message: format!("Nothing answers on port {port} in the container yet ({e}).") }, None),
            }
        }
        Request::Status => {
            reap(boxes);
            let mut containers: Vec<ContainerState> =
                boxes.iter().map(|(name, b)| ContainerState { name: name.clone(), pid: b.pid.as_raw(), running: b.running, exit_code: b.exit_code }).collect();
            containers.sort_by(|a, b| a.name.cmp(&b.name));
            reply(Reply::Status { containers }, None);
        }
        Request::Shutdown => {
            reply(Reply::Ok, None);
            return true;
        }
    }
    false
}

fn reap(boxes: &mut HashMap<String, Running>) {
    loop {
        let (pid, code) = match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(pid, code)) => (pid, code),
            Ok(WaitStatus::Signaled(pid, sig, _)) => (pid, 128 + sig as i32),
            Ok(WaitStatus::StillAlive) | Err(_) => return,
            Ok(_) => continue,
        };
        if let Some(b) = boxes.values_mut().find(|b| b.pid == pid) {
            b.running = false;
            b.exit_code = Some(code);
            b.kill_at = None;
        }
    }
}

fn stop_all(boxes: &mut HashMap<String, Running>) {
    for b in boxes.values().filter(|b| b.running) {
        let _ = kill(b.pid, Signal::SIGKILL);
    }
    std::thread::sleep(Duration::from_millis(50));
    reap(boxes);
}

// ---- Starting a container -------------------------------------------------

/// Forks the container. Returns its pid 1 as seen from the pod.
fn start(spec: &RunSpec, ctx: &Ctx) -> Result<Pid, String> {
    // The child reports its pid, or what went wrong, then the pipe closes on exec.
    let (report_r, report_w) = pipe2(OFlag::O_CLOEXEC).map_err(|e| e.desc().to_string())?;
    // SAFETY: this process is single-threaded (the pod helper), so fork is sound.
    match unsafe { fork() }.map_err(|e| e.desc().to_string())? {
        ForkResult::Child => {
            drop(report_r);
            let code = match in_new_namespaces(spec, ctx, &report_w) {
                Ok(()) => 0,
                Err(e) => {
                    let _ = fs::File::from(report_w).write_all(format!("E{e}").as_bytes());
                    127
                }
            };
            // SAFETY: leaving the forked child without running the parent's destructors.
            unsafe { nix::libc::_exit(code) }
        }
        ForkResult::Parent { child } => {
            drop(report_w);
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut fs::File::from(report_r), &mut text);
            let _ = waitpid(child, None);
            let mut pid = None;
            for line in text.split('\n').filter(|l| !l.is_empty()) {
                if let Some(p) = line.strip_prefix('P') {
                    pid = p.parse::<i32>().ok().map(Pid::from_raw);
                } else if let Some(e) = line.strip_prefix('E') {
                    if let Some(p) = pid {
                        let _ = kill(p, Signal::SIGKILL);
                    }
                    return Err(e.to_string());
                }
            }
            pid.ok_or_else(|| "The container did not start.".to_string())
        }
    }
}

/// Runs in the forked child: new namespaces, then forks the container's pid 1.
fn in_new_namespaces(spec: &RunSpec, ctx: &Ctx, report: &OwnedFd) -> Result<(), String> {
    unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWIPC | CloneFlags::CLONE_NEWPID)
        .map_err(|e| format!("Could not make the container's namespaces: {}", e.desc()))?;
    // SAFETY: still single-threaded.
    match unsafe { fork() }.map_err(|e| e.desc().to_string())? {
        ForkResult::Child => {
            let code = match pid_one(spec, ctx, report) {
                Ok(code) => code,
                Err(e) => {
                    let _ = nix::unistd::write(report, format!("E{e}\n").as_bytes());
                    127
                }
            };
            // SAFETY: as above.
            unsafe { nix::libc::_exit(code) }
        }
        ForkResult::Parent { child } => {
            nix::unistd::write(report, format!("P{}\n", child.as_raw()).as_bytes()).map_err(|e| e.desc().to_string())?;
            Ok(())
        }
    }
}

/// Opened before the root changes, used after it.
struct Carry {
    log: OwnedFd,
}

fn pid_one(spec: &RunSpec, ctx: &Ctx, report: &OwnedFd) -> Result<i32, String> {
    let carry = prepare(spec)?;
    enter_root(spec)?;
    fill_root(spec)?;
    let user = resolve_user(&spec.user, ctx.mapping)?;
    let program = find_program(spec)?;
    let env = environment(spec, &user);
    chdir(Path::new(if spec.cwd.is_empty() { "/" } else { &spec.cwd })).map_err(|e| format!("The working folder {} is missing ({}).", spec.cwd, e.desc()))?;

    // Signals wait until pid 1 is ready to pass them on.
    let mut set = SigSet::empty();
    set.add(Signal::SIGCHLD);
    for s in FORWARDED {
        set.add(s);
    }
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&set), None).map_err(|e| e.desc().to_string())?;

    // SAFETY: single-threaded.
    match unsafe { fork() }.map_err(|e| e.desc().to_string())? {
        ForkResult::Child => {
            let err = exec(&carry, &user, &program, spec, &env, &set);
            let _ = nix::unistd::write(report, format!("E{err}\n").as_bytes());
            // SAFETY: as above.
            unsafe { nix::libc::_exit(127) }
        }
        ForkResult::Parent { child } => {
            drop(carry);
            // Pid 1 never execs, so close-on-exec does not help here: it lets
            // go of the report pipe (so the pod sees the start finish) and of
            // the lifeline's write end (so only the pod keeps it open).
            // SAFETY: closing fds this process holds; their owners never drop
            // them, because pid 1 leaves with _exit.
            unsafe {
                nix::libc::close(report.as_raw_fd());
                nix::libc::close(ctx.life_w.as_raw_fd());
            }
            Ok(init_loop(child, &set, &ctx.life_r))
        }
    }
}

/// Opens the log, which lives outside the container, before the root changes.
fn prepare(spec: &RunSpec) -> Result<Carry, String> {
    let log = open(Path::new(&spec.log), OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND | OFlag::O_CLOEXEC, Mode::from_bits_truncate(0o600))
        .map_err(|e| format!("Could not open the log file ({}).", e.desc()))?;
    Ok(Carry { log })
}

fn mnt<'a>(source: Option<&'a str>, target: &str, fstype: Option<&'a str>, flags: MsFlags, data: Option<&'a str>) -> Result<(), String> {
    mount(source, target, fstype, flags, data).map_err(|e| format!("Could not mount {target}: {}", e.desc()))
}

/// Overlay on the image, the mounts that need the host (proc, sysfs,
/// devices, volumes), then pivot into it so "/" is the container's.
///
/// Host files can only be bound while the host's root is still in this
/// mount namespace, and the kernel only allows a new proc while the host's
/// is visible, so all of that happens before the pivot. Paths inside the
/// container are resolved with RESOLVE_IN_ROOT, so an image's symlinks
/// cannot send a mount to the host.
fn enter_root(spec: &RunSpec) -> Result<(), String> {
    mnt(None, "/", None, MsFlags::MS_REC | MsFlags::MS_PRIVATE, None)?;
    let opts = format!("lowerdir={},upperdir={},workdir={}", spec.rootfs, spec.upper, spec.work);
    mnt(Some("overlay"), &spec.merged, Some("overlay"), MsFlags::empty(), Some(&opts))?;
    let merged = Path::new(&spec.merged);
    let nosuid = MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC;

    let proc_dir = real_dir(&merged.join("proc"))?;
    mnt(Some("proc"), &proc_dir, Some("proc"), nosuid, None)?;
    let sys_dir = real_dir(&merged.join("sys"))?;
    // Not every kernel allows sysfs here; programs rarely need it.
    let _ = mnt(Some("sysfs"), &sys_dir, Some("sysfs"), nosuid | MsFlags::MS_RDONLY, None);

    // A fresh tmpfs for /dev, so nothing in it comes from the image.
    let dev = real_dir(&merged.join("dev"))?;
    mnt(Some("tmpfs"), &dev, Some("tmpfs"), MsFlags::MS_NOSUID | MsFlags::MS_STRICTATIME, Some("mode=755,size=65536k"))?;
    for d in DEVICES {
        let host = format!("/dev/{d}");
        if !Path::new(&host).exists() {
            continue;
        }
        let at = format!("{dev}/{d}");
        fs::File::create(&at).map_err(|e| format!("Could not make {at} ({e})."))?;
        mnt(Some(&host), &at, None, MsFlags::MS_BIND, None)?;
    }

    let root = open(merged, OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC, Mode::empty()).map_err(|e| e.desc().to_string())?;
    for m in &spec.mounts {
        let target = inside_path(&m.target)?;
        let is_dir = fs::metadata(&m.source).map_err(|e| format!("Could not use {} for the container ({e}).", m.source))?.is_dir();
        let at = open_in_root(&root, &target, is_dir)?;
        let at_path = format!("/proc/self/fd/{}", at.as_raw_fd());
        mnt(Some(&m.source), &at_path, None, MsFlags::MS_BIND | MsFlags::MS_REC, None)?;
        if m.read_only {
            // Reopen: the old fd still points under the new mount.
            let top = open_in_root(&root, &target, is_dir)?;
            let top_path = format!("/proc/self/fd/{}", top.as_raw_fd());
            let ro = MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY;
            // Some host folders carry flags that must be kept when remounting.
            mnt(None, &top_path, None, ro, None).or_else(|_| mnt(None, &top_path, None, ro | nosuid, None))?;
        }
    }
    drop(root);

    chdir(merged).map_err(|e| e.desc().to_string())?;
    // Stacking the old root on top of the new one avoids needing a folder for it.
    pivot_root(".", ".").map_err(|e| format!("Could not switch to the container's files: {}", e.desc()))?;
    umount2(".", MntFlags::MNT_DETACH).map_err(|e| e.desc().to_string())?;
    chdir("/").map_err(|e| e.desc().to_string())
}

/// What is left once "/" is the container's: ptys, shared memory, the
/// host name and the hosts file.
fn fill_root(spec: &RunSpec) -> Result<(), String> {
    let nosuid = MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC;
    for (link, to) in [("/dev/fd", "/proc/self/fd"), ("/dev/stdin", "/proc/self/fd/0"), ("/dev/stdout", "/proc/self/fd/1"), ("/dev/stderr", "/proc/self/fd/2")]
    {
        let _ = std::os::unix::fs::symlink(to, link);
    }
    let _ = fs::create_dir_all("/dev/pts");
    if mnt(Some("devpts"), "/dev/pts", Some("devpts"), MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC, Some("newinstance,ptmxmode=0666,mode=0620")).is_ok() {
        let _ = std::os::unix::fs::symlink("pts/ptmx", "/dev/ptmx");
    }
    let _ = fs::create_dir_all("/dev/shm");
    mnt(Some("shm"), "/dev/shm", Some("tmpfs"), nosuid, Some("mode=1777,size=65536k"))?;
    let _ = fs::create_dir_all("/tmp");

    sethostname(&spec.name).map_err(|e| e.desc().to_string())?;
    let _ = fs::create_dir_all("/etc");
    let names = std::iter::once(spec.name.as_str()).chain(spec.hosts.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
    write_file("/etc/hosts", &format!("127.0.0.1\tlocalhost {names}\n::1\tlocalhost\n"))?;
    write_file("/etc/resolv.conf", "# Crane: this container has no network outside its stack.\n")?;
    Ok(())
}

/// Opens `target` inside `root`, making missing folders (and the last
/// part as a file when `dir` is false). Every symlink on the way resolves
/// as if `root` were "/", so nothing leads out. Returns an O_PATH fd.
fn open_in_root(root: &OwnedFd, target: &Path, dir: bool) -> Result<OwnedFd, String> {
    let how = || OpenHow::new().flags(OFlag::O_PATH | OFlag::O_CLOEXEC).resolve(ResolveFlag::RESOLVE_IN_ROOT | ResolveFlag::RESOLVE_NO_MAGICLINKS);
    let fail = |what: &str, e: nix::Error| format!("Could not {what} {} in the container ({}).", target.display(), e.desc());
    let parts: Vec<&std::ffi::OsStr> = target
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    let mut so_far = PathBuf::new();
    for (i, name) in parts.iter().enumerate() {
        let next = so_far.join(name);
        let last = i + 1 == parts.len();
        match openat2(root, &next, how()) {
            Ok(fd) if last => return Ok(fd),
            Ok(_) => {}
            Err(nix::errno::Errno::ENOENT) => {
                let parent = if so_far.as_os_str().is_empty() {
                    root.try_clone().map_err(|e| e.to_string())?
                } else {
                    openat2(root, &so_far, how()).map_err(|e| fail("open", e))?
                };
                if last && !dir {
                    nix::fcntl::openat(
                        &parent,
                        *name,
                        OFlag::O_CREAT | OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                        Mode::from_bits_truncate(0o644),
                    )
                    .map_err(|e| fail("make", e))?;
                } else {
                    nix::sys::stat::mkdirat(&parent, *name, Mode::from_bits_truncate(0o755)).map_err(|e| fail("make", e))?;
                }
                if last {
                    return openat2(root, &next, how()).map_err(|e| fail("open", e));
                }
            }
            Err(e) => return Err(fail("open", e)),
        }
        so_far = next;
    }
    root.try_clone().map_err(|e| e.to_string())
}

/// A real folder at `path` inside the container's files. Used before the
/// root changes, so an image's symlink here could point at the host: it is
/// replaced by a plain folder instead of being followed.
fn real_dir(path: &Path) -> Result<String, String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            fs::remove_file(path).map_err(|e| format!("Could not replace {} ({e}).", path.display()))?;
            fs::create_dir(path).map_err(|e| e.to_string())?;
        }
        Err(_) => fs::create_dir(path).map_err(|e| format!("Could not make {} ({e}).", path.display()))?,
    }
    Ok(path.display().to_string())
}

fn write_file(path: &str, text: &str) -> Result<(), String> {
    // An image may have these as symlinks; write a plain file instead.
    let _ = fs::remove_file(path);
    fs::write(path, text).map_err(|e| format!("Could not write {path} ({e})."))
}

/// An absolute path without `..`, for mount targets in the container.
fn inside_path(target: &str) -> Result<PathBuf, String> {
    let p = Path::new(target);
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("\"{target}\" must be an absolute path in the container, without \"..\"."));
    }
    Ok(p.to_path_buf())
}

struct User {
    uid: u32,
    gid: u32,
    home: String,
}

/// `""`, `name`, `uid`, `name:group` or `uid:gid`, looked up in the container's files.
fn resolve_user(spec: &str, mapping: Mapping) -> Result<User, String> {
    let (u, g) = spec.split_once(':').map_or((spec, None), |(u, g)| (u, Some(g)));
    let passwd = fs::read_to_string("/etc/passwd").unwrap_or_default();
    let entry = |pred: &dyn Fn(&[&str]) -> bool| passwd.lines().map(|l| l.split(':').collect::<Vec<_>>()).find(|f| f.len() >= 6 && pred(f));
    let (uid, mut gid, home) = if u.is_empty() || u == "root" || u == "0" {
        (0, 0, "/root".to_string())
    } else if let Ok(n) = u.parse::<u32>() {
        let found = entry(&|f| f[2] == u);
        (n, found.as_ref().and_then(|f| f[3].parse().ok()).unwrap_or(n), found.map(|f| f[5].to_string()).unwrap_or_else(|| "/".into()))
    } else {
        let f = entry(&|f| f[0] == u).ok_or_else(|| format!("The image wants to run as \"{u}\", but that user is not in its /etc/passwd."))?;
        (f[2].parse().unwrap_or(0), f[3].parse().unwrap_or(0), f[5].to_string())
    };
    if let Some(g) = g {
        gid = match g.parse::<u32>() {
            Ok(n) => n,
            Err(_) => fs::read_to_string("/etc/group")
                .unwrap_or_default()
                .lines()
                .map(|l| l.split(':').collect::<Vec<_>>())
                .find(|f| f.len() >= 3 && f[0] == g)
                .and_then(|f| f[2].parse().ok())
                .ok_or_else(|| format!("The group \"{g}\" is not in the image's /etc/group."))?,
        };
    }
    if mapping == Mapping::Single && (uid != 0 || gid != 0) {
        return Err(format!(
            "The image runs as user {uid}, which needs a range of ids for your user in /etc/subuid and /etc/subgid (for example `yourname:100000:65536`)."
        ));
    }
    if uid >= userns::SUB_IDS || gid >= userns::SUB_IDS {
        return Err(format!("The image runs as user {uid}, above the {} ids Crane maps.", userns::SUB_IDS));
    }
    Ok(User { uid, gid, home })
}

fn env_value<'a>(env: &'a [String], key: &str) -> Option<&'a str> {
    env.iter().rev().find_map(|kv| kv.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

fn find_program(spec: &RunSpec) -> Result<PathBuf, String> {
    let first = spec.args.first().ok_or("The image has no command to run; give it one.")?;
    if first.contains('/') {
        return Ok(PathBuf::from(first));
    }
    let path = env_value(&spec.env, "PATH").unwrap_or(DEFAULT_PATH);
    path.split(':')
        .map(|dir| Path::new(dir).join(first))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .ok_or_else(|| format!("\"{first}\" was not found in the image (looked in {path})."))
}

fn environment(spec: &RunSpec, user: &User) -> Vec<String> {
    let mut env = spec.env.clone();
    if env_value(&env, "PATH").is_none() {
        env.push(format!("PATH={DEFAULT_PATH}"));
    }
    if env_value(&env, "HOME").is_none() {
        env.push(format!("HOME={}", user.home));
    }
    if env_value(&env, "HOSTNAME").is_none() {
        env.push(format!("HOSTNAME={}", spec.name));
    }
    env
}

/// The program's side of the last fork. Only returns on failure.
fn exec(carry: &Carry, user: &User, program: &Path, spec: &RunSpec, env: &[String], blocked: &SigSet) -> String {
    let fail = |what: &str, e: nix::Error| format!("{what}: {}", e.desc());
    if let Err(e) = nix::unistd::dup2_stdout(&carry.log).and_then(|_| nix::unistd::dup2_stderr(&carry.log)) {
        return fail("Could not send output to the log", e);
    }
    if let Ok(null) = open("/dev/null", OFlag::O_RDONLY, Mode::empty()) {
        let _ = nix::unistd::dup2_stdin(&null);
    }
    let gid = Gid::from_raw(user.gid);
    // Fails harmlessly when only one id is mapped (setgroups is denied then).
    let _ = setgroups(&[gid]);
    if let Err(e) = setgid(gid) {
        return fail("Could not switch group", e);
    }
    if let Err(e) = setuid(Uid::from_raw(user.uid)) {
        return fail("Could not switch user", e);
    }
    let _ = sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(blocked), None);
    let to_c = |s: &str| CString::new(s).map_err(|_| format!("\"{s}\" has a zero byte."));
    let args: Result<Vec<CString>, String> = spec.args.iter().map(|a| to_c(a)).collect();
    let envs: Result<Vec<CString>, String> = env.iter().map(|a| to_c(a)).collect();
    let (args, envs, prog) = match (args, envs, to_c(&program.to_string_lossy())) {
        (Ok(a), Ok(e), Ok(p)) => (a, e, p),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return e,
    };
    match execve(&prog, &args, &envs) {
        Err(e) => format!("Could not start {}: {}", program.display(), e.desc()),
        Ok(never) => match never {},
    }
}

/// Pid 1 of the container: passes signals on and reaps orphans, like tini,
/// and ends the container if the pod goes away. Exits with the program's
/// code, which ends the whole container.
fn init_loop(program: Pid, set: &SigSet, lifeline: &OwnedFd) -> i32 {
    let Ok(signals) = SignalFd::with_flags(set, SfdFlags::SFD_CLOEXEC) else {
        let _ = kill(program, Signal::SIGKILL);
        return 1;
    };
    loop {
        let mut fds = [PollFd::new(signals.as_fd(), PollFlags::POLLIN), PollFd::new(lifeline.as_fd(), PollFlags::POLLIN)];
        if poll(&mut fds, PollTimeout::NONE).is_err() {
            continue;
        }
        if fds[1].revents().is_some_and(|r| !r.is_empty()) {
            let _ = kill(program, Signal::SIGKILL);
            return 137;
        }
        let Ok(Some(info)) = signals.read_signal() else { continue };
        match Signal::try_from(info.ssi_signo as i32) {
            Ok(Signal::SIGCHLD) => loop {
                match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(pid, code)) if pid == program => return code,
                    Ok(WaitStatus::Signaled(pid, sig, _)) if pid == program => return 128 + sig as i32,
                    Ok(WaitStatus::StillAlive) | Err(_) => break,
                    Ok(_) => continue,
                }
            },
            Ok(sig) => {
                let _ = kill(program, sig);
            }
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_targets_stay_simple() {
        assert!(inside_path("/var/lib/data").is_ok());
        assert!(inside_path("relative/path").is_err());
        assert!(inside_path("/var/../../etc").is_err());
    }

    #[test]
    fn finds_env_values_last_one_wins() {
        let env = vec!["PATH=/a".to_string(), "X=1".into(), "PATH=/b".into()];
        assert_eq!(env_value(&env, "PATH"), Some("/b"));
        assert_eq!(env_value(&env, "PAT"), None);
    }
}
