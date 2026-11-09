//! Certificates and proxies against real TLS servers on 127.0.0.1, with a
//! throwaway CA made for each test.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crane_core::network::Network;
use crane_core::{RequestSpec, SendError, SendOptions, send};
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{RootCertStore, ServerConfig, crypto};

struct Pki {
    ca: CertifiedIssuer<'static, KeyPair>,
    dir: tempfile::TempDir,
}

impl Pki {
    fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        let pki = Self { ca, dir: tempfile::tempdir().unwrap() };
        std::fs::write(pki.path("ca.pem"), pki.ca.pem()).unwrap();
        pki
    }

    fn path(&self, name: &str) -> String {
        self.dir.path().join(name).display().to_string()
    }

    /// A certificate signed by the CA, as (DER chain, key).
    fn issue(&self, names: &[&str]) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>, String) {
        let key = KeyPair::generate().unwrap();
        let cert = CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>()).unwrap().signed_by(&key, &self.ca).unwrap();
        let pem = format!("{}{}", cert.pem(), key.serialize_pem());
        (vec![cert.der().clone()], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())), pem)
    }

    /// Serves one HTTPS request with "hello"; `require_client` asks for a client certificate.
    async fn serve(&self, require_client: bool) -> String {
        let provider = Arc::new(crypto::aws_lc_rs::default_provider());
        let (chain, key, _) = self.issue(&["localhost"]);
        let builder = ServerConfig::builder_with_provider(Arc::clone(&provider)).with_safe_default_protocol_versions().unwrap();
        let config = if require_client {
            let mut roots = RootCertStore::empty();
            roots.add(self.ca.der().clone()).unwrap();
            let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider).build().unwrap();
            builder.with_client_cert_verifier(verifier).with_single_cert(chain, key).unwrap()
        } else {
            builder.with_no_client_auth().with_single_cert(chain, key).unwrap()
        };
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(tcp).await else { return };
                    let mut buf = [0u8; 4096];
                    let _ = tls.read(&mut buf).await;
                    let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        format!("https://localhost:{port}/")
    }
}

fn get(url: &str) -> RequestSpec {
    RequestSpec { method: "GET".into(), url: url.into(), headers: vec![], body: None }
}

fn with(network: Network) -> SendOptions {
    SendOptions { timeout: Duration::from_secs(5), network, ..SendOptions::default() }
}

#[tokio::test]
async fn an_unknown_ca_is_refused_with_a_hint() {
    let pki = Pki::new();
    let url = pki.serve(false).await;
    let err = send(&get(&url), &with(Network::default())).await.unwrap_err();
    assert!(matches!(err, SendError::Tls(..)), "{err:?}");
    assert!(err.to_string().contains("add it to the environment under Network"));
}

#[tokio::test]
async fn trusts_a_ca_from_a_file() {
    let pki = Pki::new();
    let url = pki.serve(false).await;
    let res = send(&get(&url), &with(Network { ca_file: pki.path("ca.pem"), ..Network::default() })).await.unwrap();
    assert_eq!((res.status, res.body.as_deref()), (200, Some("hello")));
}

#[tokio::test]
async fn can_skip_the_check_outside_production() {
    let pki = Pki::new();
    let url = pki.serve(false).await;
    let res = send(&get(&url), &with(Network { insecure: true, ..Network::default() })).await.unwrap();
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn sends_a_client_certificate_when_the_server_wants_one() {
    let pki = Pki::new();
    let url = pki.serve(true).await;
    let ca_only = Network { ca_file: pki.path("ca.pem"), ..Network::default() };
    assert!(send(&get(&url), &with(ca_only.clone())).await.is_err());

    let (_, _, pem) = pki.issue(&["crane-client"]);
    std::fs::write(pki.path("client.pem"), pem).unwrap();
    let res = send(&get(&url), &with(Network { client_cert_file: pki.path("client.pem"), ..ca_only })).await.unwrap();
    assert_eq!(res.body.as_deref(), Some("hello"));
}

#[tokio::test]
async fn explains_unusable_certificate_files() {
    let pki = Pki::new();
    std::fs::write(pki.path("empty.pem"), "not a certificate").unwrap();
    let missing = send(&get("https://localhost:1/"), &with(Network { ca_file: pki.path("nope.pem"), ..Network::default() })).await.unwrap_err();
    assert!(matches!(&missing, SendError::BadCertFile(_, why) if why.contains("no such file")), "{missing:?}");
    let empty = send(&get("https://localhost:1/"), &with(Network { ca_file: pki.path("empty.pem"), ..Network::default() })).await.unwrap_err();
    assert!(empty.to_string().contains("no PEM certificates"), "{empty}");
    let key = send(&get("https://localhost:1/"), &with(Network { client_cert_file: pki.path("ca.pem"), ..Network::default() })).await.unwrap_err();
    assert!(key.to_string().contains("unencrypted private key"), "{key}");
}

#[tokio::test]
async fn goes_through_the_proxy() {
    // A proxy gets the full address in the request line.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
        let body = line.trim();
        let mut stream = stream;
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let proxy = Network { proxy: format!("http://127.0.0.1:{port}"), ..Network::default() };
    let res = send(&get("http://api.internal.example/users"), &with(proxy)).await.unwrap();
    assert_eq!(res.body.as_deref(), Some("GET http://api.internal.example/users HTTP/1.1"));
}
