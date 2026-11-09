//! Builds the HTTP client for one send, so each answer comes with its own
//! timing: how long the name lookup took and how long connecting (with TLS)
//! took. A fresh client also means no connection is reused, which is what
//! you want when you are measuring an API.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tower_layer::Layer;
use tower_service::Service;

/// Time spent before the request could be written, summed over redirects.
#[derive(Debug, Default, Clone, Copy)]
pub struct Phases {
    pub dns: Duration,
    /// The whole connection setup: name lookup, TCP and TLS.
    pub connect_total: Duration,
}

pub type SharedPhases = Arc<Mutex<Phases>>;

fn add(phases: &SharedPhases, f: impl FnOnce(&mut Phases)) {
    if let Ok(mut p) = phases.lock() {
        f(&mut p);
    }
}

/// The system resolver, with a stopwatch.
struct TimedResolver(SharedPhases);

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let phases = Arc::clone(&self.0);
        Box::pin(async move {
            let started = Instant::now();
            let found = tokio::net::lookup_host((name.as_str(), 0)).await;
            add(&phases, |p| p.dns += started.elapsed());
            let addrs: Vec<SocketAddr> = found?.collect();
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Wraps reqwest's connector to time each new connection.
#[derive(Clone)]
struct TimingLayer(SharedPhases);

impl<S> Layer<S> for TimingLayer {
    type Service = Timed<S>;
    fn layer(&self, inner: S) -> Timed<S> {
        Timed { inner, phases: Arc::clone(&self.0) }
    }
}

#[derive(Clone)]
struct Timed<S> {
    inner: S,
    phases: SharedPhases,
}

impl<S, R> Service<R> for Timed<S>
where
    S: Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        let started = Instant::now();
        let connecting = self.inner.call(req);
        let phases = Arc::clone(&self.phases);
        Box::pin(async move {
            let conn = connecting.await;
            add(&phases, |p| p.connect_total += started.elapsed());
            conn
        })
    }
}

/// A client that records its phases into the returned handle.
pub fn timed_builder(user_agent: &str) -> (reqwest::ClientBuilder, SharedPhases) {
    let phases = SharedPhases::default();
    let builder = reqwest::Client::builder()
        .user_agent(user_agent)
        .pool_max_idle_per_host(0)
        .dns_resolver(TimedResolver(Arc::clone(&phases)))
        .connector_layer(TimingLayer(Arc::clone(&phases)));
    (builder, phases)
}
