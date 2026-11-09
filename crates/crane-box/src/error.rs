use thiserror::Error;

/// What went wrong, said so a person can act on it.
#[derive(Debug, Error)]
pub enum BoxError {
    #[error("{0}")]
    BadInput(String),
    #[error("Could not reach the registry {0}: {1}")]
    Network(String, String),
    #[error("The registry said no to {0}: {1}")]
    Registry(String, String),
    #[error("{0} has no image for {1}. Crane runs Linux images built for this processor.")]
    NoPlatform(String, String),
    #[error("A download for {0} was damaged (the checksum did not match). Try again.")]
    Checksum(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("Containers need Linux user namespaces: {0}")]
    Namespaces(String),
    #[error("{0}")]
    Runtime(String),
    #[error("Could not read or write Crane's container files: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BoxError>;
