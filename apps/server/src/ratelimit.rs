//! In-memory fixed-window rate limiting per client IP (single API replica by design).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<(String, IpAddr), (Instant, u32)>>,
}

impl RateLimiter {
    /// Returns false when the caller exceeded `max` requests in `window`.
    #[cfg(test)]
    pub fn check(&self, bucket: &str, ip: IpAddr, max: u32, window: Duration) -> bool {
        self.wait(bucket, ip, max, window).is_none()
    }

    /// `None` when the request may go on, else the whole seconds until the window resets.
    pub fn wait(&self, bucket: &str, ip: IpAddr, max: u32, window: Duration) -> Option<u64> {
        let now = Instant::now();
        let mut windows = self.windows.lock().expect("rate limiter lock");
        if windows.len() > 50_000 {
            windows
                .retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(86_400));
        }
        let entry = windows.entry((bucket.to_string(), ip)).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 <= max {
            return None;
        }
        let left = window.saturating_sub(now.duration_since(entry.0));
        Some(left.as_secs() + u64::from(left.subsec_nanos() > 0))
    }
}

/// Client IP honouring `TRUST_PROXY` hops (nginx in front of the API).
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: u8) -> IpAddr {
    let fallback = peer.map(|p| p.ip()).unwrap_or(IpAddr::from([127, 0, 0, 1]));
    if trust_proxy == 0 {
        return fallback;
    }
    let forwarded: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    // Trust `trust_proxy` hops: the socket peer is hop 0, the last forwarded entry hop 1...
    let index = forwarded.len().checked_sub(trust_proxy as usize);
    index
        .and_then(|i| forwarded.get(i).copied())
        .or_else(|| forwarded.first().copied())
        .unwrap_or(fallback)
}

/// Route-group limit configuration.
#[derive(Clone, Copy)]
pub struct Limit {
    pub bucket: &'static str,
    pub max: u32,
    pub window: Duration,
}

pub const fn per_minute(bucket: &'static str, max: u32) -> Limit {
    Limit {
        bucket,
        max,
        window: Duration::from_secs(60),
    }
}
pub const fn per_hour(bucket: &'static str, max: u32) -> Limit {
    Limit {
        bucket,
        max,
        window: Duration::from_secs(3600),
    }
}
pub const fn per_day(bucket: &'static str, max: u32) -> Limit {
    Limit {
        bucket,
        max,
        window: Duration::from_secs(86_400),
    }
}

pub async fn enforce(
    State((state, limit)): State<(AppState, Limit)>,
    request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let ip = client_ip(request.headers(), peer, state.config.trust_proxy);
    if let Some(seconds) = state
        .limiter
        .wait(limit.bucket, ip, limit.max, limit.window)
    {
        return ApiError::rate_limited_for(seconds).into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_bucket() {
        let l = RateLimiter::default();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        for _ in 0..3 {
            assert!(l.check("a", ip, 3, Duration::from_secs(60)));
        }
        assert!(!l.check("a", ip, 3, Duration::from_secs(60)));
        assert!(l.check("b", ip, 3, Duration::from_secs(60)));
    }

    #[test]
    fn says_how_long_to_wait() {
        let l = RateLimiter::default();
        let ip: IpAddr = "10.0.0.2".parse().unwrap();
        assert_eq!(l.wait("w", ip, 1, Duration::from_secs(60)), None);
        let left = l.wait("w", ip, 1, Duration::from_secs(60)).unwrap();
        assert!((59..=60).contains(&left), "{left}");
        let response = ApiError::rate_limited_for(left).into_response();
        assert_eq!(response.status(), 429);
        assert_eq!(
            response.headers()[axum::http::header::RETRY_AFTER],
            left.to_string().as_str()
        );
    }

    #[test]
    fn trusts_one_proxy_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2".parse().unwrap());
        assert_eq!(client_ip(&h, None, 1), "2.2.2.2".parse::<IpAddr>().unwrap());
        assert_eq!(
            client_ip(&h, None, 0),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
    }
}
