//! Pulls images from OCI and Docker registries: anonymous token login,
//! manifest (or index) for this machine's platform, then the config and
//! layer blobs, each checked against its sha256 digest.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::{BoxError, Result};
use crate::image_ref::ImageRef;

const ACCEPT: &str = "application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, \
application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    #[serde(default)]
    pub media_type: String,
    pub digest: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub platform: Option<Platform>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
}

/// Either an index (one manifest per platform) or a manifest.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fetched {
    #[serde(default)]
    manifests: Vec<Descriptor>,
    #[serde(default)]
    config: Option<Descriptor>,
    #[serde(default)]
    layers: Vec<Descriptor>,
}

/// The image for this machine: its config blob and its layers, in order.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub digest: String,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

/// The registry's name for this processor.
pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => other,
    }
}

/// `Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="..."`
pub fn parse_challenge(header: &str) -> Option<(String, Vec<(String, String)>)> {
    let rest = header.trim().strip_prefix("Bearer ")?;
    let mut realm = None;
    let mut params = Vec::new();
    for part in rest.split(',') {
        let (k, v) = part.split_once('=')?;
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "realm" => realm = Some(v),
            "error" => {}
            key => params.push((key.to_string(), v)),
        }
    }
    Some((realm?, params))
}

fn pick_platform(list: &[Descriptor]) -> Option<&Descriptor> {
    list.iter().find(|d| d.platform.as_ref().is_some_and(|p| p.os == "linux" && p.architecture == host_arch()))
}

pub fn is_valid_digest(d: &str) -> bool {
    d.strip_prefix("sha256:").is_some_and(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
}

pub struct Registry {
    http: reqwest::Client,
    image: ImageRef,
    token: Option<String>,
}

impl Registry {
    pub fn new(image: ImageRef) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Crane/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| BoxError::Network(image.registry.clone(), e.to_string()))?;
        Ok(Self { http, image, token: None })
    }

    fn url(&self, path: &str) -> String {
        format!("{}://{}/v2/{}/{path}", self.image.scheme(), self.image.api_host(), self.image.repository)
    }

    fn net(&self, e: reqwest::Error) -> BoxError {
        BoxError::Network(self.image.registry.clone(), e.without_url().to_string())
    }

    /// GET with the token, logging in once if the registry asks for it.
    async fn get(&mut self, url: &str, accept: Option<&str>) -> Result<reqwest::Response> {
        for attempt in 0..2 {
            let mut req = self.http.get(url);
            if let Some(a) = accept {
                req = req.header(reqwest::header::ACCEPT, a);
            }
            if let Some(t) = &self.token {
                req = req.bearer_auth(t);
            }
            let res = req.send().await.map_err(|e| self.net(e))?;
            if res.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                let challenge = res.headers().get(reqwest::header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
                self.login(&challenge).await?;
                continue;
            }
            if !res.status().is_success() {
                let status = res.status();
                let what = if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::UNAUTHORIZED {
                    format!("{status}. Check the name and tag; private images are not supported yet.")
                } else {
                    status.to_string()
                };
                return Err(BoxError::Registry(self.image.to_string(), what));
            }
            return Ok(res);
        }
        Err(BoxError::Registry(self.image.to_string(), "login did not work".into()))
    }

    async fn login(&mut self, challenge: &str) -> Result<()> {
        let Some((realm, params)) = parse_challenge(challenge) else {
            return Err(BoxError::Registry(self.image.to_string(), "it wants a login Crane does not support yet".into()));
        };
        #[derive(Deserialize)]
        struct Token {
            token: Option<String>,
            access_token: Option<String>,
        }
        let url = reqwest::Url::parse_with_params(&realm, &params).map_err(|e| BoxError::Registry(self.image.to_string(), format!("odd login address ({e})")))?;
        if url.scheme() != "https" && self.image.scheme() == "https" {
            return Err(BoxError::Registry(self.image.to_string(), "it wants a login over plain HTTP".into()));
        }
        let res = self.http.get(url).send().await.map_err(|e| self.net(e))?;
        if !res.status().is_success() {
            return Err(BoxError::Registry(self.image.to_string(), format!("login failed ({})", res.status())));
        }
        let t: Token = res.json().await.map_err(|e| self.net(e))?;
        self.token = t.token.or(t.access_token);
        Ok(())
    }

    /// The manifest for linux on this processor, following an index if there is one.
    pub async fn manifest(&mut self) -> Result<Manifest> {
        let reference = self.image.reference.clone();
        let (digest, fetched) = self.fetch_manifest(&reference).await?;
        if fetched.config.is_some() {
            return finish(digest, fetched, &self.image);
        }
        let Some(chosen) = pick_platform(&fetched.manifests).cloned() else {
            return Err(BoxError::NoPlatform(self.image.to_string(), format!("linux/{}", host_arch())));
        };
        let (digest, fetched) = self.fetch_manifest(&chosen.digest).await?;
        finish(digest, fetched, &self.image)
    }

    async fn fetch_manifest(&mut self, reference: &str) -> Result<(String, Fetched)> {
        let res = self.get(&self.url(&format!("manifests/{reference}")), Some(ACCEPT)).await?;
        let bytes = res.bytes().await.map_err(|e| self.net(e))?;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        if reference.starts_with("sha256:") && digest != reference {
            return Err(BoxError::Checksum(self.image.to_string()));
        }
        let fetched = serde_json::from_slice(&bytes).map_err(|e| BoxError::Registry(self.image.to_string(), format!("unreadable manifest ({e})")))?;
        Ok((digest, fetched))
    }

    /// Downloads a blob into `dir` (named by its digest) unless it is there.
    /// `progress` gets the bytes of this blob so far.
    pub async fn blob(&mut self, d: &Descriptor, dir: &Path, mut progress: impl FnMut(u64)) -> Result<PathBuf> {
        if !is_valid_digest(&d.digest) {
            return Err(BoxError::Registry(self.image.to_string(), format!("odd digest {}", d.digest)));
        }
        let hex = &d.digest["sha256:".len()..];
        let path = dir.join(hex);
        if path.exists() {
            progress(d.size);
            return Ok(path);
        }
        tokio::fs::create_dir_all(dir).await?;
        let mut res = self.get(&self.url(&format!("blobs/{}", d.digest)), None).await?;
        let tmp = dir.join(format!("{hex}.part"));
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        while let Some(chunk) = res.chunk().await.map_err(|e| self.net(e))? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            progress(done);
        }
        file.flush().await?;
        drop(file);
        if format!("{:x}", hasher.finalize()) != hex {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(BoxError::Checksum(self.image.to_string()));
        }
        tokio::fs::rename(&tmp, &path).await?;
        Ok(path)
    }
}

fn finish(digest: String, f: Fetched, image: &ImageRef) -> Result<Manifest> {
    let config = f.config.ok_or_else(|| BoxError::Registry(image.to_string(), "the manifest has no config".into()))?;
    for l in &f.layers {
        if l.media_type.contains("zstd") {
            return Err(BoxError::Unsupported(format!("{image} uses zstd-compressed layers, which Crane cannot unpack yet.")));
        }
        if l.media_type.contains("nondistributable") || l.media_type.contains("foreign") {
            return Err(BoxError::Unsupported(format!("{image} has Windows layers; Crane runs Linux images.")));
        }
    }
    Ok(Manifest { digest, config, layers: f.layers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_login_challenge() {
        let (realm, params) =
            parse_challenge(r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#).unwrap();
        assert_eq!(realm, "https://auth.docker.io/token");
        assert_eq!(params, [("service".into(), "registry.docker.io".into()), ("scope".into(), "repository:library/alpine:pull".into())]);
        assert!(parse_challenge("Basic realm=x").is_none());
    }

    #[test]
    fn picks_linux_on_this_processor() {
        let index: Fetched = serde_json::from_str(&format!(
            r#"{{"manifests":[
                {{"digest":"sha256:{a}","platform":{{"architecture":"arm64","os":"linux"}}}},
                {{"digest":"sha256:{b}","platform":{{"architecture":"{arch}","os":"windows"}}}},
                {{"digest":"sha256:{c}","platform":{{"architecture":"{arch}","os":"linux"}}}}
            ]}}"#,
            a = "a".repeat(64),
            b = "b".repeat(64),
            c = "c".repeat(64),
            arch = host_arch()
        ))
        .unwrap();
        assert_eq!(pick_platform(&index.manifests).unwrap().digest, format!("sha256:{}", "c".repeat(64)));
    }

    #[test]
    fn checks_digests() {
        assert!(is_valid_digest(&format!("sha256:{}", "0f".repeat(32))));
        assert!(!is_valid_digest("sha256:../../etc/passwd"));
        assert!(!is_valid_digest(&format!("sha512:{}", "0".repeat(64))));
    }
}
