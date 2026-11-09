//! Crane's own container engine. No Docker or Podman: images are pulled
//! straight from registries and run rootless in Linux namespaces.

pub mod error;
pub mod image;
pub mod image_ref;
pub mod proto;
pub mod registry;

#[cfg(target_os = "linux")]
pub mod engine;
#[cfg(target_os = "linux")]
pub mod helper;
#[cfg(target_os = "linux")]
mod pod;
#[cfg(target_os = "linux")]
pub mod unpack;
#[cfg(target_os = "linux")]
pub mod userns;

pub use error::{BoxError, Result};
