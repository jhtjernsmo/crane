//! Crane's request engine. The Tauri shell only passes a [`RequestSpec`] in and
//! a [`Response`] (or a [`SendError`] with a message for people) back out.

mod client;
pub mod collection;
pub mod env;
pub mod history;
pub mod http_file;
pub mod secret_scan;
mod send;
mod url;
pub mod vars;

pub use send::{MAX_SHOWN_BYTES, Response, SendOptions, Timing, mask_response, send};
pub use url::normalize_url;

use serde::{Deserialize, Serialize};

/// What the user wants to send.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// Why a request could not be sent. The messages are shown as they are, so
/// they say what happened and what to try, not what the library called it.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SendError {
    #[error("Type an address to send to, like https://api.github.com.")]
    EmptyUrl,
    #[error("\"{0}\" is not an address Crane understands. Check for typos or spaces.")]
    BadUrl(String),
    #[error("\"{0}\" is not an HTTP method.")]
    BadMethod(String),
    #[error("The header \"{0}\" has a name or value that is not allowed.")]
    BadHeader(String),
    #[error("Could not find {0}. Check the address, or your network connection.")]
    HostNotFound(String),
    #[error("Nothing answered at {0}. Is the server running, and on that port?")]
    Refused(String),
    #[error("{0} did not answer within {1} seconds.")]
    TimedOut(String, u64),
    #[error("The secure connection to {0} failed: {1}")]
    Tls(String, String),
    #[error("The request failed: {0}")]
    Other(String),
    #[error("This request uses {}, but no environment is picked. Pick one at the top, or add one under Environments.", list_vars(.0))]
    NoEnvironment(Vec<String>),
    #[error("{} has no {}. Add {} under Environments, or pick another environment.", .0, list_vars(.1), if .1.len() == 1 { "it" } else { "them" })]
    MissingVars(String, Vec<String>),
    #[error("The secret {{{{{1}}}}} in {0} has no value yet. Set it under Environments.")]
    SecretNotSet(String, String),
    #[error("Could not read the system keyring: {0}. Is it unlocked?")]
    Keyring(String),
}

/// `{{a}}`, `{{a}} or {{b}}`, `{{a}}, {{b}} or {{c}}`.
fn list_vars(names: &[String]) -> String {
    let wrapped: Vec<String> = names.iter().map(|n| format!("{{{{{n}}}}}")).collect();
    match wrapped.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}
