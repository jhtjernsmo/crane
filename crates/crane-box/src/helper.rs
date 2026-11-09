//! Crane runs itself again as a helper for the parts that need a fresh,
//! single-threaded process inside a user namespace: unpacking images,
//! removing files owned by mapped ids, and running a pod.
//!
//! Call [`run_if_helper`] first thing in `main`. It returns at once in a
//! normal start, and never returns in a helper run.

use std::os::fd::{FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use crate::{pod, unpack, userns};

/// The first argument that marks a helper run.
pub const HELPER_ARG: &str = "__crane-box";
/// The fd number the pod's control socket is given in a helper run.
pub const CONTROL_FD: i32 = 3;

pub fn run_if_helper() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some(HELPER_ARG) {
        return;
    }
    let code = match (args.get(2).map(String::as_str), &args[3.min(args.len())..]) {
        (Some("unpack"), [dest, layers @ ..]) => report(enter().and_then(|_| {
            let paths: Vec<PathBuf> = layers.iter().map(PathBuf::from).collect();
            unpack::unpack(Path::new(dest), &paths).map(|r| format!("{{\"skippedDevices\":{}}}", r.skipped_devices)).map_err(|e| e.to_string())
        })),
        (Some("remove"), [base, targets @ ..]) => report(enter().and_then(|_| remove(Path::new(base), targets))),
        (Some("pod"), []) => {
            // SAFETY: Crane put the control socket on this fd before starting us.
            let control = unsafe { OwnedFd::from_raw_fd(CONTROL_FD) };
            pod::run(control)
        }
        _ => {
            eprintln!("crane: unknown helper call");
            2
        }
    };
    std::process::exit(code)
}

fn enter() -> Result<(), String> {
    userns::enter().map(|_| ()).map_err(|e| e.to_string())
}

/// Prints the outcome as one JSON line for the caller.
fn report(r: Result<String, String>) -> i32 {
    match r {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            println!("{}", serde_json::json!({ "error": e }));
            1
        }
    }
}

/// Removes folders, but only below `base` (Crane's own data folder).
fn remove(base: &Path, targets: &[String]) -> Result<String, String> {
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    for t in targets {
        let Ok(path) = Path::new(t).canonicalize() else { continue };
        if !path.starts_with(&base) || path == base {
            return Err(format!("{t} is outside Crane's folder."));
        }
        std::fs::remove_dir_all(&path).map_err(|e| format!("Could not remove {t}: {e}"))?;
    }
    Ok("{}".into())
}
