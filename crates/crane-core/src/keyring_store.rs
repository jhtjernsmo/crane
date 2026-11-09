//! Secret values in the system keyring: the Secret Service (GNOME Keyring,
//! KWallet) on Linux, Keychain on macOS, Credential Manager on Windows.
//!
//! Each secret is one entry, service `dev.crane.app`, user `<env id>/<name>`,
//! so it shows up readable in the system's own keyring viewer.

use crate::env::{SecretError, SecretStore};
use keyring::{Entry, Error};

const SERVICE: &str = "dev.crane.app";

pub struct KeyringSecrets;

fn entry(env_id: &str, name: &str) -> Result<Entry, SecretError> {
    Entry::new(SERVICE, &format!("{env_id}/{name}")).map_err(explain)
}

fn explain(e: Error) -> SecretError {
    SecretError(match e {
        Error::NoDefaultStore => "no system keyring was found (on Linux, install and start GNOME Keyring or KWallet)".into(),
        Error::NoStorageAccess(_) => "the keyring is locked or Crane was not allowed in".into(),
        other => other.to_string(),
    })
}

impl SecretStore for KeyringSecrets {
    fn get(&self, env_id: &str, name: &str) -> Result<Option<String>, SecretError> {
        match entry(env_id, name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(explain(e)),
        }
    }

    fn set(&self, env_id: &str, name: &str, value: &str) -> Result<(), SecretError> {
        entry(env_id, name)?.set_password(value).map_err(explain)
    }

    fn delete(&self, env_id: &str, name: &str) -> Result<(), SecretError> {
        match entry(env_id, name)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(explain(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to the real keyring, so it only runs when asked:
    /// `cargo test -p crane-core --features keyring -- --ignored`
    #[test]
    #[ignore]
    fn round_trip_in_the_system_keyring() {
        let store = KeyringSecrets;
        let (env, name) = ("crane-selftest", "probe");
        store.set(env, name, "value-1").unwrap();
        assert_eq!(store.get(env, name).unwrap().as_deref(), Some("value-1"));
        store.delete(env, name).unwrap();
        assert_eq!(store.get(env, name).unwrap(), None);
        store.delete(env, name).unwrap();
    }
}
