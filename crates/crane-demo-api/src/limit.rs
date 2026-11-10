//! A per-address request limit with one-minute windows, kept in memory.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;

const WINDOW: Duration = Duration::from_secs(60);
/// Past this many tracked addresses, finished windows are dropped.
const PRUNE_AT: usize = 10_000;

pub struct RateLimit {
    per_window: u32,
    hits: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl RateLimit {
    pub fn new(per_minute: u32) -> Self {
        Self { per_window: per_minute, hits: Mutex::new(HashMap::new()) }
    }

    /// `Err` holds the seconds until the address may try again.
    pub fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if hits.len() >= PRUNE_AT {
            hits.retain(|_, (started, _)| now.duration_since(*started) < WINDOW);
        }
        let entry = hits.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        if entry.1 >= self.per_window {
            let left = WINDOW.saturating_sub(now.duration_since(entry.0));
            return Err(left.as_secs().max(1));
        }
        entry.1 += 1;
        Ok(())
    }
}

/// The caller's address. Behind the proxy, which is the only thing that can
/// reach this server, the proxy writes the real one as the last
/// `X-Forwarded-For` entry; anything before it came from the client and is
/// not trusted.
pub fn client_ip(peer: SocketAddr, headers: &HeaderMap, trust_proxy: bool) -> IpAddr {
    if !trust_proxy {
        return peer.ip();
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|last| last.trim().parse().ok())
        .unwrap_or(peer.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_the_limit() {
        let limit = RateLimit::new(2);
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        assert!(limit.check(ip).is_ok());
        assert!(limit.check(ip).is_ok());
        assert!(limit.check(ip).is_err());
        assert!(limit.check("192.0.2.2".parse().unwrap()).is_ok());
    }

    #[test]
    fn trusts_only_the_last_forwarded_entry_and_only_behind_the_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.7".parse().unwrap());
        let proxy: SocketAddr = "172.18.0.2:5000".parse().unwrap();
        assert_eq!(client_ip(proxy, &headers, true), "203.0.113.7".parse::<IpAddr>().unwrap());
        assert_eq!(client_ip(proxy, &headers, false), proxy.ip());
        assert_eq!(client_ip(proxy, &HeaderMap::new(), true), proxy.ip());
    }
}
