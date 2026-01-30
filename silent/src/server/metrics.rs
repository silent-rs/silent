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
pub fn record_http3_read_timeout() {
    counter!("silent.server.http3.read_timeout").increment(1);
}

#[cfg(feature = "quic")]
pub fn record_http3_response_size(bytes: u64) {
    histogram!("silent.server.http3.response_bytes").record(bytes as f64);
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
pub fn record_webtransport_session_duration(dur_ns: u64) {
    histogram!("silent.server.webtransport.session_ns").record(dur_ns as f64);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_metrics_default() {
        let metrics = ServerMetrics::default();
        assert_eq!(
            metrics.accept_ok.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .accept_err
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .handler_ok
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .handler_err
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn test_server_metrics_singleton() {
        let metrics1 = server_metrics();
        let metrics2 = server_metrics();

        // 验证返回的是同一个实例
        assert!(std::ptr::eq(metrics1, metrics2));
    }

    #[test]
    fn test_record_accept_ok() {
        // 重置计数器
        let metrics = server_metrics();
        metrics
            .accept_ok
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_accept_ok();
        assert_eq!(
            metrics.accept_ok.load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        record_accept_ok();
        record_accept_ok();
        assert_eq!(
            metrics.accept_ok.load(std::sync::atomic::Ordering::Relaxed),
            3
        );
    }

    #[test]
    fn test_record_accept_err() {
        let metrics = server_metrics();
        metrics
            .accept_err
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_accept_err();
        assert_eq!(
            metrics
                .accept_err
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        record_accept_err();
        assert_eq!(
            metrics
                .accept_err
                .load(std::sync::atomic::Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn test_record_handler_ok() {
        let metrics = server_metrics();
        metrics
            .handler_ok
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_handler_ok();
        assert_eq!(
            metrics
                .handler_ok
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        record_handler_ok();
        record_handler_ok();
        assert_eq!(
            metrics
                .handler_ok
                .load(std::sync::atomic::Ordering::Relaxed),
            3
        );
    }

    #[test]
    fn test_record_handler_err() {
        let metrics = server_metrics();
        metrics
            .handler_err
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_handler_err();
        assert_eq!(
            metrics
                .handler_err
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        record_handler_err();
        assert_eq!(
            metrics
                .handler_err
                .load(std::sync::atomic::Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn test_record_handler_timeout() {
        let metrics = server_metrics();
        metrics
            .handler_timeout
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_handler_timeout();
        assert_eq!(
            metrics
                .handler_timeout
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        record_handler_timeout();
        record_handler_timeout();
        assert_eq!(
            metrics
                .handler_timeout
                .load(std::sync::atomic::Ordering::Relaxed),
            3
        );
    }

    #[test]
    fn test_record_rate_limiter_closed() {
        let metrics = server_metrics();
        metrics
            .rate_limiter_closed
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_rate_limiter_closed();
        assert_eq!(
            metrics
                .rate_limiter_closed
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_record_rate_limiter_timeout() {
        let metrics = server_metrics();
        metrics
            .rate_limiter_timeout
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_rate_limiter_timeout();
        assert_eq!(
            metrics
                .rate_limiter_timeout
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_record_graceful_shutdown() {
        let metrics = server_metrics();
        metrics
            .graceful_shutdowns
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_graceful_shutdown();
        assert_eq!(
            metrics
                .graceful_shutdowns
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_record_forced_shutdown() {
        let metrics = server_metrics();
        metrics
            .forced_shutdowns
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_forced_shutdown();
        assert_eq!(
            metrics
                .forced_shutdowns
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_record_handler_duration() {
        // 测试记录处理时长（仅验证不会 panic）
        record_handler_duration(1000);
        record_handler_duration(5000);
        record_handler_duration(10_000_000);
    }

    #[test]
    fn test_record_wait_duration() {
        // 测试记录等待时长（仅验证不会 panic）
        record_wait_duration(100);
        record_wait_duration(500);
        record_wait_duration(1000);
    }

    #[test]
    fn test_record_shutdown_duration() {
        // 测试记录关停时长（仅验证不会 panic）
        record_shutdown_duration("phase1", 1000);
        record_shutdown_duration("phase2", 5000);
        record_shutdown_duration("cleanup", 10_000);
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_datagram_metrics_noop() {
        super::record_webtransport_datagram_dropped();
        super::record_webtransport_rate_limited();
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_http3_body_oversize() {
        let metrics = server_metrics();
        metrics
            .http3_body_oversize
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_http3_body_oversize();
        assert_eq!(
            metrics
                .http3_body_oversize
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_http3_read_timeout() {
        // 测试 HTTP3 读取超时记录（仅验证不会 panic）
        record_http3_read_timeout();
        record_http3_read_timeout();
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_http3_response_size() {
        // 测试 HTTP3 响应大小记录（仅验证不会 panic）
        record_http3_response_size(1024);
        record_http3_response_size(4096);
        record_http3_response_size(10_000);
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_accept() {
        let metrics = server_metrics();
        metrics
            .webtransport_accept
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_webtransport_accept();
        assert_eq!(
            metrics
                .webtransport_accept
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_error() {
        let metrics = server_metrics();
        metrics
            .webtransport_error
            .store(0, std::sync::atomic::Ordering::Relaxed);

        record_webtransport_error();
        assert_eq!(
            metrics
                .webtransport_error
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_handshake_duration() {
        // 测试 WebTransport 握手时长记录（仅验证不会 panic）
        record_webtransport_handshake_duration(1000);
        record_webtransport_handshake_duration(5000);
        record_webtransport_handshake_duration(10_000_000);
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_session_duration() {
        // 测试 WebTransport 会话时长记录（仅验证不会 panic）
        record_webtransport_session_duration(1000);
        record_webtransport_session_duration(5000);
        record_webtransport_session_duration(10_000_000);
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_datagram_dropped() {
        // 测试 WebTransport datagram 丢弃记录（仅验证不会 panic）
        record_webtransport_datagram_dropped();
        record_webtransport_datagram_dropped();
    }

    #[test]
    #[cfg(feature = "quic")]
    fn test_record_webtransport_rate_limited() {
        // 测试 WebTransport 速率限制记录（仅验证不会 panic）
        record_webtransport_rate_limited();
        record_webtransport_rate_limited();
    }

    #[test]
    fn test_metrics_multiple_counters() {
        // 测试多个计数器同时增加
        record_accept_ok();
        record_handler_ok();
        record_graceful_shutdown();

        let metrics = server_metrics();
        assert_eq!(
            metrics.accept_ok.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .handler_ok
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .graceful_shutdowns
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_metrics_debug() {
        let metrics = ServerMetrics::default();
        // 验证 Debug trait 实现
        let debug_str = format!("{:?}", metrics);
        assert!(debug_str.contains("ServerMetrics"));
    }
}
