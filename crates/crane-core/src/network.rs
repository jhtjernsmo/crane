//! Per-environment network settings for internal APIs: an extra CA to trust
//! (a company CA or a self-signed server), a client certificate for mutual
//! TLS, and a proxy.

use reqwest::{Certificate, ClientBuilder, Identity, NoProxy, Proxy};
use serde::{Deserialize, Serialize};

use crate::SendError;

/// The proxy setting that means "connect directly, ignore HTTP_PROXY".
pub const NO_PROXY: &str = "none";
const PROXY_SCHEMES: [&str; 4] = ["http://", "https://", "socks5://", "socks5h://"];

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    /// PEM file with CA certificates to trust on top of the system's.
    #[serde(default)]
    pub ca_file: String,
    /// PEM file with a client certificate and its unencrypted private key.
    #[serde(default)]
    pub client_cert_file: String,
    /// Skip certificate checks. Never allowed in production environments.
    #[serde(default)]
    pub insecure: bool,
    /// Empty: the system's proxy settings. `none`: connect directly. Else a
    /// proxy URL, which may use `{{variables}}` for a password.
    #[serde(default)]
    pub proxy: String,
    /// Hosts that skip the proxy, comma separated.
    #[serde(default)]
    pub no_proxy: String,
}

impl Network {
    pub fn validate(&self, env_name: &str, production: bool) -> Result<(), String> {
        if production && self.insecure {
            return Err(format!("{env_name} is a production environment, so it always checks certificates. Add the CA file instead."));
        }
        let proxy = self.proxy.trim();
        if !proxy.is_empty() && proxy != NO_PROXY && !PROXY_SCHEMES.iter().any(|s| proxy.to_ascii_lowercase().starts_with(s)) {
            return Err(format!("The proxy in {env_name} should start with http://, https:// or socks5://."));
        }
        Ok(())
    }

    /// Applies the settings to a client being built. Files are read here, on
    /// every send, so a renewed certificate is picked up without a restart.
    pub async fn apply(&self, mut builder: ClientBuilder) -> Result<ClientBuilder, SendError> {
        if !self.ca_file.trim().is_empty() {
            let pem = read(&self.ca_file).await?;
            let certs = Certificate::from_pem_bundle(&pem).map_err(|_| bad_file(&self.ca_file, "it has no PEM certificates"))?;
            if certs.is_empty() {
                return Err(bad_file(&self.ca_file, "it has no PEM certificates"));
            }
            for c in certs {
                builder = builder.add_root_certificate(c);
            }
        }
        if !self.client_cert_file.trim().is_empty() {
            let pem = read(&self.client_cert_file).await?;
            let id = Identity::from_pem(&pem)
                .map_err(|_| bad_file(&self.client_cert_file, "it needs a certificate and an unencrypted private key, both in PEM"))?;
            builder = builder.identity(id);
        }
        if self.insecure {
            builder = builder.tls_danger_accept_invalid_certs(true);
        }
        let proxy = self.proxy.trim();
        if proxy == NO_PROXY {
            builder = builder.no_proxy();
        } else if !proxy.is_empty() {
            let p = Proxy::all(proxy).map_err(|_| SendError::BadProxy)?.no_proxy(NoProxy::from_string(&self.no_proxy));
            builder = builder.proxy(p);
        }
        Ok(builder)
    }
}

async fn read(path: &str) -> Result<Vec<u8>, SendError> {
    tokio::fs::read(path.trim()).await.map_err(|e| bad_file(path, &e.to_string().to_lowercase()))
}

fn bad_file(path: &str, why: &str) -> SendError {
    SendError::BadCertFile(path.trim().to_string(), why.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_always_checks_certificates() {
        let n = Network { insecure: true, ..Network::default() };
        assert!(n.validate("Local", false).is_ok());
        assert!(n.validate("Prod", true).unwrap_err().contains("always checks certificates"));
    }

    #[test]
    fn proxies_need_a_scheme() {
        let with = |p: &str| Network { proxy: p.into(), ..Network::default() }.validate("Local", false);
        assert!(with("").is_ok());
        assert!(with("none").is_ok());
        assert!(with("http://proxy.corp:3128").is_ok());
        assert!(with("socks5://127.0.0.1:1080").is_ok());
        assert!(with("http://me:{{proxy_password}}@proxy:3128").is_ok());
        assert!(with("proxy.corp:3128").is_err());
    }
}
