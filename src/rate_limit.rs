//! Per-client rate limiting (GCRA via `governor`), usable on any router:
//!
//! ```ignore
//! let limiter = RateLimit::per_minute(10, false);
//! let router = Router::new().route(..).layer(middleware::from_fn_with_state(limiter, rate_limit::enforce));
//! ```

use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    state::keyed::DefaultKeyedStateStore,
};

use crate::error::AppError;

type Limiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

#[derive(Clone)]
pub struct RateLimit {
    limiter: Arc<Limiter>,
    trust_proxy_headers: bool,
}

impl RateLimit {
    /// Allows a burst of `n` requests, replenishing evenly over a minute.
    pub fn per_minute(n: u32, trust_proxy_headers: bool) -> Self {
        let quota = Quota::per_minute(NonZeroU32::new(n.max(1)).expect("n >= 1"));
        Self { limiter: Arc::new(RateLimiter::keyed(quota)), trust_proxy_headers }
    }

    /// Drops state for clients that are back at full quota. Run periodically.
    pub fn spawn_cleanup(&self, every: Duration) {
        let limiter = self.limiter.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                limiter.retain_recent();
                limiter.shrink_to_fit();
            }
        });
    }

    fn client_key(&self, req: &Request) -> IpAddr {
        let forwarded = self
            .trust_proxy_headers
            .then(|| {
                req.headers()
                    .get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split(',').next())
                    .and_then(|v| v.trim().parse::<IpAddr>().ok())
            })
            .flatten();
        let ip = forwarded
            .or_else(|| req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip()))
            .unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        match ip {
            // Rate-limit IPv6 clients per /64 so rotating addresses inside a prefix doesn't help.
            IpAddr::V6(v6) => {
                let mut o = v6.octets();
                o[8..].fill(0);
                IpAddr::V6(Ipv6Addr::from(o))
            }
            v4 => v4,
        }
    }

    fn check(&self, key: IpAddr) -> Result<(), u64> {
        self.limiter
            .check_key(&key)
            .map_err(|not_until| not_until.wait_time_from(DefaultClock::default().now()).as_secs().max(1))
    }
}

/// Middleware: responds `429` with `Retry-After` once the client exceeds its quota.
pub async fn enforce(State(rl): State<RateLimit>, req: Request, next: Next) -> Response {
    match rl.check(rl.client_key(&req)) {
        Ok(()) => next.run(req).await,
        Err(retry_after_secs) => AppError::TooManyRequests { retry_after_secs }.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, http::StatusCode, middleware, routing::get};
    use tower::ServiceExt;

    use super::*;

    fn app(limit: u32, trust: bool) -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(RateLimit::per_minute(limit, trust), enforce))
    }

    fn req(ip: &str, xff: Option<&str>) -> Request {
        let mut r = Request::builder().uri("/");
        if let Some(x) = xff {
            r = r.header("x-forwarded-for", x);
        }
        let mut r = r.body(Body::empty()).unwrap();
        r.extensions_mut().insert(ConnectInfo(SocketAddr::new(ip.parse().unwrap(), 1234)));
        r
    }

    #[tokio::test]
    async fn blocks_after_quota_with_retry_after() {
        let app = app(2, false);
        for _ in 0..2 {
            assert_eq!(app.clone().oneshot(req("1.1.1.1", None)).await.unwrap().status(), 200);
        }
        let res = app.clone().oneshot(req("1.1.1.1", None)).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().contains_key("retry-after"));
        // Other clients are unaffected.
        assert_eq!(app.oneshot(req("2.2.2.2", None)).await.unwrap().status(), 200);
    }

    #[tokio::test]
    async fn forwarded_header_is_only_used_when_trusted() {
        let untrusted = app(1, false);
        untrusted.clone().oneshot(req("1.1.1.1", Some("9.9.9.1"))).await.unwrap();
        let res = untrusted.oneshot(req("1.1.1.1", Some("9.9.9.2"))).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);

        let trusted = app(1, true);
        trusted.clone().oneshot(req("1.1.1.1", Some("9.9.9.1"))).await.unwrap();
        let res = trusted.oneshot(req("1.1.1.1", Some("9.9.9.2"))).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
