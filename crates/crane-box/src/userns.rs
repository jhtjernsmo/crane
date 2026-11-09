//! Becoming "root" inside a new user namespace, with the user's subordinate
//! ids mapped too (via the setuid `newuidmap` helpers), so images that run
//! as other users (postgres, nginx) work. Without them, only the user's own
//! id is mapped.
//!
//! Must run in a single-threaded process: Crane re-runs itself as a helper
//! for this, never inside the app process.

use std::fs;
use std::io::{Read, Write};
use std::process::Command;

use nix::sched::{CloneFlags, unshare};
use nix::unistd::{ForkResult, Gid, Pid, Uid, fork, getgid, getpid, getuid, pipe, setresgid, setresuid};

use crate::error::{BoxError, Result};

/// The size of the id range a user gets in /etc/subuid by default.
pub const SUB_IDS: u32 = 65536;

/// `start` and `count` for `user` in /etc/subuid or /etc/subgid.
fn sub_range(file: &str, user: &str, uid: u32) -> Option<(u32, u32)> {
    let text = fs::read_to_string(file).ok()?;
    text.lines().find_map(|l| {
        let mut parts = l.split(':');
        let (who, start, count) = (parts.next()?, parts.next()?, parts.next()?);
        (who == user || who == uid.to_string()).then(|| Some((start.parse().ok()?, count.parse().ok()?)))?
    })
}

fn user_name(uid: u32) -> Option<String> {
    let text = fs::read_to_string("/etc/passwd").ok()?;
    text.lines().find_map(|l| {
        let mut p = l.split(':');
        let name = p.next()?;
        p.next()?;
        (p.next()? == uid.to_string()).then(|| name.to_string())
    })
}

/// How ids were mapped, so callers can explain what will not work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// Root plus the user's subordinate range.
    Full,
    /// Only the user's own id, as root.
    Single,
}

/// Moves this process into a new user namespace as uid 0.
pub fn enter() -> Result<Mapping> {
    let (uid, gid) = (getuid().as_raw(), getgid().as_raw());
    let name = user_name(uid).unwrap_or_default();
    let ranges = sub_range("/etc/subuid", &name, uid).zip(sub_range("/etc/subgid", &name, uid));
    let tools = which("newuidmap").zip(which("newgidmap"));

    let mapping = match (ranges, tools) {
        (Some((u, g)), Some((newuidmap, newgidmap))) => {
            let parent = getpid();
            let (ready_r, ready_w) = pipe().map_err(ns)?;
            // The child stays outside and maps us once we have unshared.
            match unsafe { fork() }.map_err(ns)? {
                ForkResult::Child => {
                    drop(ready_w);
                    let mut b = [0u8; 1];
                    let ok = fs::File::from(ready_r).read_exact(&mut b).is_ok()
                        && map(&newuidmap, parent, uid, u)
                        && map(&newgidmap, parent, gid, g);
                    unsafe { nix::libc::_exit(if ok { 0 } else { 1 }) }
                }
                ForkResult::Parent { child } => {
                    drop(ready_r);
                    unshare(CloneFlags::CLONE_NEWUSER).map_err(ns)?;
                    fs::File::from(ready_w).write_all(b"1")?;
                    let status = nix::sys::wait::waitpid(child, None).map_err(ns)?;
                    if !matches!(status, nix::sys::wait::WaitStatus::Exited(_, 0)) {
                        return Err(BoxError::Namespaces("newuidmap could not map the subordinate ids in /etc/subuid".into()));
                    }
                    Mapping::Full
                }
            }
        }
        _ => {
            unshare(CloneFlags::CLONE_NEWUSER).map_err(ns)?;
            fs::write("/proc/self/setgroups", "deny")?;
            fs::write("/proc/self/uid_map", format!("0 {uid} 1"))?;
            fs::write("/proc/self/gid_map", format!("0 {gid} 1"))?;
            Mapping::Single
        }
    };
    setresgid(Gid::from_raw(0), Gid::from_raw(0), Gid::from_raw(0)).map_err(ns)?;
    setresuid(Uid::from_raw(0), Uid::from_raw(0), Uid::from_raw(0)).map_err(ns)?;
    Ok(mapping)
}

fn map(tool: &std::path::Path, pid: Pid, id: u32, (start, count): (u32, u32)) -> bool {
    let count = count.min(SUB_IDS);
    Command::new(tool)
        .args([pid.to_string(), "0".into(), id.to_string(), "1".into(), "1".into(), start.to_string(), count.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

fn which(tool: &str) -> Option<std::path::PathBuf> {
    ["/usr/bin", "/bin", "/usr/sbin", "/usr/local/bin"].iter().map(|d| std::path::Path::new(d).join(tool)).find(|p| p.exists())
}

fn ns(e: nix::Error) -> BoxError {
    BoxError::Namespaces(e.desc().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_subordinate_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("subuid");
        fs::write(&f, "alice:100000:65536\n1001:200000:1000\n").unwrap();
        let f = f.to_str().unwrap();
        assert_eq!(sub_range(f, "alice", 1000), Some((100000, 65536)));
        assert_eq!(sub_range(f, "bob", 1001), Some((200000, 1000)));
        assert_eq!(sub_range(f, "carol", 1002), None);
    }
}
