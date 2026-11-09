//! What an image says about how to run it, and the record Crane keeps of
//! each pulled image.

use serde::{Deserialize, Serialize};

use crate::error::{BoxError, Result};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    pub entrypoint: Vec<String>,
    pub cmd: Vec<String>,
    /// `KEY=value` pairs.
    pub env: Vec<String>,
    pub working_dir: String,
    /// `name`, `uid`, `uid:gid` or `name:group`; empty means root.
    pub user: String,
    pub exposed_ports: Vec<u16>,
}

impl ImageConfig {
    /// Reads the `config` part of an OCI or Docker image config blob.
    pub fn from_blob(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Blob {
            #[serde(default)]
            config: Option<Inner>,
            #[serde(default)]
            os: String,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "PascalCase")]
        struct Inner {
            entrypoint: Option<Vec<String>>,
            cmd: Option<Vec<String>>,
            env: Option<Vec<String>>,
            working_dir: Option<String>,
            user: Option<String>,
            exposed_ports: Option<std::collections::BTreeMap<String, serde_json::Value>>,
        }
        let blob: Blob = serde_json::from_slice(bytes).map_err(|e| BoxError::Unsupported(format!("The image config is unreadable ({e}).")))?;
        if !blob.os.is_empty() && blob.os != "linux" {
            return Err(BoxError::Unsupported(format!("This is a {} image; Crane runs Linux images.", blob.os)));
        }
        let c = blob.config.unwrap_or_default();
        Ok(Self {
            entrypoint: c.entrypoint.unwrap_or_default(),
            cmd: c.cmd.unwrap_or_default(),
            env: c.env.unwrap_or_default(),
            working_dir: c.working_dir.unwrap_or_default(),
            user: c.user.unwrap_or_default(),
            exposed_ports: c.exposed_ports.unwrap_or_default().keys().filter_map(|k| k.strip_suffix("/tcp").unwrap_or(k).parse().ok()).collect(),
        })
    }

    /// The command to start: entrypoint followed by cmd, or `args` instead of cmd.
    pub fn command(&self, args: Option<&[String]>) -> Vec<String> {
        let mut out = self.entrypoint.clone();
        out.extend(args.map(<[String]>::to_vec).unwrap_or_else(|| self.cmd.clone()));
        out
    }
}

/// A pulled image, as kept in `images/*.json`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageRecord {
    /// As the user wrote it, normalised: `postgres:16`.
    pub name: String,
    pub manifest_digest: String,
    pub config_digest: String,
    pub layers: Vec<String>,
    /// Compressed download size in bytes.
    pub size: u64,
    /// Seconds since 1970.
    pub pulled_at: u64,
    pub config: ImageConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_image_config() {
        let blob = br#"{"os":"linux","architecture":"amd64","config":{"Entrypoint":["docker-entrypoint.sh"],"Cmd":["postgres"],
            "Env":["PATH=/usr/bin","PGDATA=/var/lib/postgresql/data"],"WorkingDir":"","User":"","ExposedPorts":{"5432/tcp":{}}}}"#;
        let c = ImageConfig::from_blob(blob).unwrap();
        assert_eq!(c.command(None), ["docker-entrypoint.sh", "postgres"]);
        assert_eq!(c.command(Some(&["postgres".into(), "-c".into(), "fsync=off".into()])), ["docker-entrypoint.sh", "postgres", "-c", "fsync=off"]);
        assert_eq!(c.exposed_ports, [5432]);
        assert_eq!(c.env[1], "PGDATA=/var/lib/postgresql/data");
    }

    #[test]
    fn refuses_windows_images() {
        assert!(ImageConfig::from_blob(br#"{"os":"windows","config":{}}"#).is_err());
        assert_eq!(ImageConfig::from_blob(br#"{"os":"linux"}"#).unwrap(), ImageConfig::default());
    }
}
