use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use metrics::{counter, histogram};

/// Server 运行时指标（进程内计数），便于对接外部导出或调试。
///
/// 注意：计数为近似值，使用 `Relaxed` 语义。
#[derive(Debug, Default)]
pub struct ServerMetrics {
    pub accept_ok: AtomicU64,
    pub accept_err: AtomicU64,
    pub rate_limiter_closed: AtomicU64,
    pub rate_limiter_timeout: AtomicU64,
    pub handler_ok: AtomicU64,
    pub handler_err: AtomicU64,
    pub handler_timeout: AtomicU64,
    pub graceful_shutdowns: AtomicU64,
    pub forced_shutdowns: AtomicU64,
    #[cfg(feature = "quic")]
    pub http3_body_oversize: AtomicU64,
    #[cfg(feature = "quic")]
    pub webtransport_accept: AtomicU64,
    #[cfg(feature = "quic")]
    pub webtransport_error: AtomicU64,
}

static METRICS: OnceLock<ServerMetrics> = OnceLock::new();

pub fn server_metrics() -> &'static ServerMetrics {
    METRICS.get_or_init(ServerMetrics::default)
}

fn inc(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

pub fn record_accept_ok() {
    inc(&server_metrics().accept_ok);

    counter!("silent.server.accept.ok").increment(1);
}

pub fn record_accept_err() {
    inc(&server_metrics().accept_err);

    counter!("silent.server.accept.err").increment(1);
}

pub fn record_rate_limiter_closed() {
    inc(&server_metrics().rate_limiter_closed);

    counter!("silent.server.ratelimiter.closed").increment(1);
}

pub fn record_rate_limiter_timeout() {
    inc(&server_metrics().rate_limiter_timeout);

    counter!("silent.server.ratelimiter.timeout").increment(1);
}

pub fn record_handler_ok() {
    inc(&server_metrics().handler_ok);

    counter!("silent.server.handler.ok").increment(1);
}

pub fn record_handler_err() {
    inc(&server_metrics().handler_err);

    counter!("silent.server.handler.err").increment(1);
}

pub fn record_handler_timeout() {
    inc(&server_metrics().handler_timeout);

    counter!("silent.server.handler.timeout").increment(1);
}

pub fn record_graceful_shutdown() {
    inc(&server_metrics().graceful_shutdowns);

    counter!("silent.server.shutdown.graceful").increment(1);
}

pub fn record_forced_shutdown() {
    inc(&server_metrics().forced_shutdowns);

    counter!("silent.server.shutdown.forced").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_http3_body_oversize() {
    inc(&server_metrics().http3_body_oversize);

    counter!("silent.server.http3.body_oversize").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_webtransport_accept() {
    inc(&server_metrics().webtransport_accept);

    counter!("silent.server.webtransport.accept").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_webtransport_error() {
    inc(&server_metrics().webtransport_error);

    counter!("silent.server.webtransport.error").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_webtransport_handshake_duration(dur_ns: u64) {
    histogram!("silent.server.webtransport.handshake_ns").record(dur_ns as f64);
}

#[cfg(feature = "quic")]
pub fn record_webtransport_datagram_dropped() {
    counter!("silent.server.webtransport.datagram_dropped").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_webtransport_rate_limited() {
    counter!("silent.server.webtransport.datagram_rate_limited").increment(1);
}

pub fn record_handler_duration(handle_ns: u64) {
    histogram!("silent.server.handler.duration_ns").record(handle_ns as f64);
}

pub fn record_wait_duration(wait_ns: u64) {
    histogram!("silent.server.accept.wait_ns").record(wait_ns as f64);
}

pub fn record_shutdown_duration(tag: &'static str, dur_ns: u64) {
    histogram!(
        "silent.server.shutdown.duration_ns",
        "phase" => tag
    )
    .record(dur_ns as f64);
}
