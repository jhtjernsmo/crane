//! Messages between Crane and a pod helper, one JSON value per packet on a
//! SOCK_SEQPACKET socket. A `Connected` reply carries the connected TCP
//! socket as an SCM_RIGHTS file descriptor.

use serde::{Deserialize, Serialize};

/// Largest message; status lists and errors are far smaller.
pub const MAX_PACKET: usize = 64 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    /// Folder or file on this computer.
    pub source: String,
    /// Where it shows up in the container.
    pub target: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunSpec {
    pub name: String,
    /// The flattened image, shared read-only between containers.
    pub rootfs: String,
    /// This container's own changes, and overlayfs' scratch folder.
    pub upper: String,
    pub work: String,
    pub merged: String,
    /// stdout and stderr go here.
    pub log: String,
    pub args: Vec<String>,
    pub env: Vec<String>,
    pub cwd: String,
    pub user: String,
    pub mounts: Vec<Mount>,
    /// Extra names for 127.0.0.1 in /etc/hosts: the other containers in the pod.
    pub hosts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "op")]
pub enum Request {
    Run(Box<RunSpec>),
    Stop { name: String, grace_secs: u64 },
    Connect { port: u16 },
    Status,
    Shutdown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerState {
    pub name: String,
    pub pid: i32,
    pub running: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "reply")]
pub enum Reply {
    /// The pod is up; `full_mapping` says whether other users inside work.
    Ready { full_mapping: bool },
    Ok,
    Started { pid: i32 },
    Connected,
    Status { containers: Vec<ContainerState> },
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_have_a_stable_shape() {
        let json = serde_json::to_string(&Request::Stop { name: "db".into(), grace_secs: 10 }).unwrap();
        assert_eq!(json, r#"{"op":"stop","name":"db","graceSecs":10}"#);
        assert_eq!(serde_json::from_str::<Reply>(r#"{"reply":"started","pid":42}"#).unwrap(), Reply::Started { pid: 42 });
    }
}

#[cfg(target_os = "linux")]
pub mod wire {
    //! Sending and receiving one message (and maybe one fd) per packet.

    use std::io::{self, IoSlice, IoSliceMut};
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

    use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::MAX_PACKET;

    pub fn send<T: Serialize>(sock: BorrowedFd<'_>, msg: &T, pass: Option<RawFd>) -> io::Result<()> {
        let bytes = serde_json::to_vec(msg).map_err(io::Error::other)?;
        if bytes.len() > MAX_PACKET {
            return Err(io::Error::other("message too large"));
        }
        let fds = pass.map(|f| [f]);
        let cmsgs: Vec<ControlMessage> = fds.as_ref().map(|f| vec![ControlMessage::ScmRights(f)]).unwrap_or_default();
        sendmsg::<()>(sock.as_raw_fd(), &[IoSlice::new(&bytes)], &cmsgs, MsgFlags::MSG_NOSIGNAL, None).map_err(io::Error::from)?;
        Ok(())
    }

    /// `None` when the other side has gone away.
    pub fn recv<T: DeserializeOwned>(sock: BorrowedFd<'_>) -> io::Result<Option<(T, Option<OwnedFd>)>> {
        let mut buf = vec![0u8; MAX_PACKET];
        let mut space = nix::cmsg_space!([RawFd; 1]);
        let (len, fd) = {
            let mut iov = [IoSliceMut::new(&mut buf)];
            let msg = recvmsg::<()>(sock.as_raw_fd(), &mut iov, Some(&mut space), MsgFlags::MSG_CMSG_CLOEXEC).map_err(io::Error::from)?;
            let mut fd = None;
            for c in msg.cmsgs().map_err(io::Error::from)? {
                if let ControlMessageOwned::ScmRights(fds) = c {
                    // SAFETY: the kernel just gave us these descriptors; we own them now.
                    fd = fds.first().map(|f| unsafe { OwnedFd::from_raw_fd(*f) });
                }
            }
            (msg.bytes, fd)
        };
        if len == 0 {
            return Ok(None);
        }
        let value = serde_json::from_slice(&buf[..len]).map_err(io::Error::other)?;
        Ok(Some((value, fd)))
    }
}
