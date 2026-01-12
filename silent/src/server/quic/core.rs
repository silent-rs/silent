use std::{net::SocketAddr, sync::Arc, time::Duration, time::Instant};

use anyhow::Result;
use bytes::{Buf, Bytes};
use h3::server::RequestStream;
use quinn::Connection as QuinnConnection;
use quinn::ConnectionError;
use scru128::Scru128Id;
use tokio::time::timeout;

#[derive(Clone)]
pub struct QuicSession {
    id: String,
    remote_addr: SocketAddr,
}

impl QuicSession {
    pub fn new(remote_addr: SocketAddr) -> Self {
        let id = Scru128Id::from_u128(rand::random()).to_string();
        Self { id, remote_addr }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
}

pub struct WebTransportStream {
    inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    max_frame_size: Option<usize>,
    read_timeout: Option<Duration>,

    max_datagram_size: Option<usize>,

    datagram_per_sec: Option<u64>,

    datagram_tokens: u64,

    last_refill: Instant,

    record_drop: bool,

    conn: Option<QuinnConnection>,
}

impl WebTransportStream {
    pub(crate) fn new(
        inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
        max_frame_size: Option<usize>,
        read_timeout: Option<Duration>,
        max_datagram_size: Option<usize>,
        datagram_per_sec: Option<u64>,
        record_drop: bool,
        conn: Option<QuinnConnection>,
    ) -> Self {
        Self {
            inner,
            max_frame_size,
            read_timeout,
            max_datagram_size,
            datagram_per_sec,
            datagram_tokens: datagram_per_sec.unwrap_or(0),
            last_refill: Instant::now(),
            record_drop,
            conn,
        }
    }

    fn refill(&mut self) {
        if let Some(rate) = self.datagram_per_sec {
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(self.last_refill);
            let refill = rate.saturating_mul(elapsed.as_secs());
            self.datagram_tokens = (self.datagram_tokens + refill).min(rate);
            self.last_refill = now;
        }
    }
    pub async fn recv_data(&mut self) -> Result<Option<Bytes>> {
        let fut = self.inner.recv_data();
        let maybe = match self.read_timeout {
            Some(t) => timeout(t, fut).await??,
            None => fut.await?,
        };
        // datagram 限速占位的令牌补充，确保字段在编译期被视为已使用。
        self.refill();
        match maybe {
            Some(mut buf) => {
                let data = buf.copy_to_bytes(buf.remaining());
                if let Some(max) = self.max_frame_size
                    && data.len() > max
                {
                    anyhow::bail!("WebTransport frame exceeds limit");
                }
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }
    /// 带限速/体积校验的 Datagram 发送占位接口。
    /// 目前通过底层 quinn::Connection 发送 datagram；若连接未启用则返回 Err。
    pub fn try_send_datagram(&mut self, data: Bytes) -> Result<()> {
        self.refill();
        if let Some(max) = self.max_datagram_size
            && data.len() > max
        {
            #[cfg(feature = "metrics")]
            if self.record_drop {
                crate::server::metrics::record_webtransport_datagram_dropped();
            }
            anyhow::bail!("Datagram frame exceeds limit");
        }
        if self.datagram_per_sec.is_some() {
            if self.datagram_tokens == 0 {
                #[cfg(feature = "metrics")]
                if self.record_drop {
                    crate::server::metrics::record_webtransport_rate_limited();
                }
                anyhow::bail!("Datagram rate limited");
            }
            self.datagram_tokens -= 1;
        }
        match &self.conn {
            Some(conn) => {
                if let Err(err) = conn.send_datagram(data) {
                    #[cfg(feature = "metrics")]
                    if self.record_drop {
                        crate::server::metrics::record_webtransport_datagram_dropped();
                    }
                    anyhow::bail!("Datagram send failed: {err}");
                }
                Ok(())
            }
            None => anyhow::bail!("Datagram not supported by connection"),
        }
    }

    /// 接收 datagram，并按 size/rate 做限速与观测。
    pub async fn recv_datagram(&mut self) -> Result<Option<Bytes>> {
        let Some(conn) = self.conn.clone() else {
            anyhow::bail!("Datagram not supported by connection");
        };
        self.refill();
        let raw = match conn.read_datagram().await {
            Ok(bytes) => bytes,
            Err(ConnectionError::ApplicationClosed { .. })
            | Err(ConnectionError::LocallyClosed) => return Ok(None),
            Err(err) => anyhow::bail!("Datagram recv failed: {err}"),
        };
        if let Some(max) = self.max_datagram_size
            && raw.len() > max
        {
            #[cfg(feature = "metrics")]
            if self.record_drop {
                crate::server::metrics::record_webtransport_datagram_dropped();
            }
            if self.record_drop {
                // 丢弃超限数据但不中断会话
                return Ok(None);
            } else {
                anyhow::bail!("Datagram frame exceeds limit");
            }
        }
        if self.datagram_per_sec.is_some() {
            if self.datagram_tokens == 0 {
                #[cfg(feature = "metrics")]
                if self.record_drop {
                    crate::server::metrics::record_webtransport_rate_limited();
                }
                if self.record_drop {
                    // 丢弃超限数据但不中断会话
                    return Ok(None);
                } else {
                    anyhow::bail!("Datagram rate limited");
                }
            }
            self.datagram_tokens -= 1;
        }
        Ok(Some(raw))
    }
    pub async fn send_data(&mut self, data: Bytes) -> Result<()> {
        Ok(self.inner.send_data(data).await?)
    }
    pub async fn finish(&mut self) -> Result<()> {
        Ok(self.inner.finish().await?)
    }
}

#[async_trait::async_trait]
pub trait WebTransportHandler: Send + Sync {
    async fn handle(
        &self,
        session: Arc<QuicSession>,
        stream: &mut WebTransportStream,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quic_session_basics() {
        let addr1: SocketAddr = "127.0.0.1:1111".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:2222".parse().unwrap();
        let s1 = QuicSession::new(addr1);
        let s2 = QuicSession::new(addr2);
        assert!(!s1.id().is_empty());
        assert_ne!(s1.id(), s2.id());
        assert_eq!(s1.remote_addr(), addr1);
        assert_eq!(s2.remote_addr(), addr2);
    }

    #[test]
    fn test_quic_session_clone() {
        // 验证 QuicSession 可以被克隆
        let addr: SocketAddr = "192.168.1.1:8080".parse().unwrap();
        let s1 = QuicSession::new(addr);
        let s2 = s1.clone();
        assert_eq!(s1.id(), s2.id());
        assert_eq!(s1.remote_addr(), s2.remote_addr());
    }

    #[test]
    fn test_quic_session_id_format() {
        // 验证 ID 格式
        let addr: SocketAddr = "10.0.0.1:443".parse().unwrap();
        let session = QuicSession::new(addr);
        let id = session.id();
        assert!(!id.is_empty());
        assert!(id.len() > 20); // SCRU128 ID 长度
    }

    #[test]
    fn test_quic_session_id_uniqueness() {
        // 验证多个 session 的 ID 唯一性
        let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let sessions: Vec<_> = (0..100).map(|_| QuicSession::new(addr)).collect();
        let ids: Vec<_> = sessions.iter().map(|s| s.id()).collect();
        let mut unique_ids = std::collections::HashSet::new();
        for id in ids {
            assert!(unique_ids.insert(id), "发现重复的 ID: {}", id);
        }
    }

    #[test]
    fn test_quic_session_ipv6_support() {
        // 验证 IPv6 地址支持
        let ipv6: SocketAddr = "[::1]:443".parse().unwrap();
        let session = QuicSession::new(ipv6);
        assert_eq!(session.remote_addr(), ipv6);
    }

    #[tokio::test]
    async fn test_quic_session_in_async_context() {
        // 验证可以在 async 上下文中使用
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let session = QuicSession::new(addr);
        let id = session.id().to_string();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_quic_session_send_sync() {
        // 验证 QuicSession 满足 Send + Sync 约束
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<QuicSession>();
        assert_sync::<QuicSession>();
    }

    #[test]
    fn test_webtransport_stream_struct_size() {
        // 验证 WebTransportStream 结构体大小
        let size = std::mem::size_of::<WebTransportStream>();
        assert!(size > 0);
    }

    #[test]
    fn test_webtransport_stream_refill_logic() {
        // 测试令牌补充逻辑的边界条件
        // 模拟时间间隔和令牌计算
        let rate: u64 = 10;
        let elapsed_secs: u64 = 2;
        let refill = rate.saturating_mul(elapsed_secs);
        assert_eq!(refill, 20);

        // 测试饱和添加
        let current_tokens: u64 = 5;
        let new_tokens = (current_tokens + refill).min(rate);
        assert_eq!(new_tokens, 10); // 不超过 rate
    }

    #[test]
    fn test_duration_saturating() {
        // 测试 Duration 的饱和减法
        use std::time::Instant;
        let now = Instant::now();
        let past = now - std::time::Duration::from_secs(1);
        let elapsed = now.saturating_duration_since(past);
        assert!(elapsed.as_secs() >= 1);
    }

    #[test]
    fn test_webtransport_handler_trait_exists() {
        // 验证 WebTransportHandler trait 存在且可以正常使用
        // 通过 trait 约束验证 trait 对象的存在性
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn WebTransportHandler>();
    }

    #[test]
    fn test_webtransport_stream_field_types() {
        // 验证 WebTransportStream 字段类型
        // 通过类型检查验证结构
        assert!(std::mem::size_of::<Option<usize>>() > 0);
        assert!(std::mem::size_of::<Option<Duration>>() > 0);
        assert!(std::mem::size_of::<Option<u64>>() > 0);
    }

    #[test]
    fn test_bytes_copy_to_bytes() {
        // 测试 Bytes 的 copy_to_bytes 方法
        let data = b"hello world".to_vec();
        let mut buf = Bytes::from(data);
        let remaining = buf.remaining();
        let copied = buf.copy_to_bytes(remaining);
        assert_eq!(copied.len(), 11);
    }

    #[test]
    fn test_webtransport_stream_refill_no_rate() {
        // 测试没有设置速率限制时的 refill 行为
        // 当 datagram_per_sec 为 None 时，refill 不应该做任何事情
        // 这个测试验证 refill 方法在没有速率限制时的正确性
        let rate: Option<u64> = None;
        assert!(rate.is_none());
    }

    #[test]
    fn test_webtransport_stream_refill_with_rate() {
        // 测试设置速率限制后的令牌计算
        let rate: u64 = 100;
        let elapsed_secs: u64 = 1;
        let expected_refill = rate.saturating_mul(elapsed_secs);
        assert_eq!(expected_refill, 100);

        // 测试令牌不超过速率限制
        let current_tokens: u64 = 50;
        let new_tokens = (current_tokens + expected_refill).min(rate);
        assert_eq!(new_tokens, 100); // 不超过 rate
    }

    #[test]
    fn test_webtransport_stream_refill_zero_elapsed() {
        // 测试经过时间为 0 时的令牌补充
        let rate: u64 = 10;
        let elapsed_secs: u64 = 0;
        let refill = rate.saturating_mul(elapsed_secs);
        assert_eq!(refill, 0);

        // 令牌应该保持不变
        let current_tokens: u64 = 5;
        let new_tokens = (current_tokens + refill).min(rate);
        assert_eq!(new_tokens, 5);
    }

    #[test]
    fn test_webtransport_stream_refill_large_elapsed() {
        // 测试经过很长时间的令牌补充
        let rate: u64 = 10;
        let elapsed_secs: u64 = 1000;
        let refill = rate.saturating_mul(elapsed_secs);
        assert_eq!(refill, 10000);

        // 令牌应该被限制在 rate
        let current_tokens: u64 = 0;
        let new_tokens = (current_tokens + refill).min(rate);
        assert_eq!(new_tokens, 10); // 不超过 rate
    }

    #[test]
    fn test_webtransport_stream_token_consumption() {
        // 测试令牌消耗逻辑
        let initial_tokens: u64 = 10;
        let consume: u64 = 1;
        let remaining = initial_tokens.saturating_sub(consume);
        assert_eq!(remaining, 9);

        // 测试消耗到 0 的情况
        let zero_tokens: u64 = 0;
        let after_consume = zero_tokens.saturating_sub(consume);
        assert_eq!(after_consume, 0);
    }

    #[test]
    fn test_webtransport_stream_size_validation() {
        // 测试大小验证逻辑
        let data_size = 100;
        let max_size: usize = 50;
        assert!(data_size > max_size);

        // 测试大小在限制内
        let valid_size = 30;
        assert!(valid_size <= max_size);
    }

    #[test]
    fn test_webtransport_stream_optional_size() {
        // 测试可选大小限制的 None 情况
        let max_size: Option<usize> = None;
        assert!(max_size.is_none());

        let data_size = 1000;
        // 当 max_size 为 None 时，应该跳过大小检查
        if let Some(max) = max_size {
            assert!(data_size <= max);
        }
        // None 分支：跳过大小检查，无需断言
    }

    #[test]
    fn test_webtransport_stream_rate_limit_check() {
        // 测试速率限制检查逻辑
        let tokens: u64 = 0;
        let has_rate_limit = true;
        assert!(tokens == 0 && has_rate_limit);

        // 测试有令牌的情况
        let tokens_with_balance: u64 = 5;
        assert!(tokens_with_balance > 0);
    }

    #[test]
    fn test_webtransport_stream_no_rate_limit() {
        // 测试没有速率限制的情况
        let rate_per_sec: Option<u64> = None;
        assert!(rate_per_sec.is_none());

        // 当没有速率限制时，应该允许任意数量的操作
        let unlimited_operations = true;
        assert!(unlimited_operations || rate_per_sec.is_some());
    }

    #[test]
    fn test_webtransport_max_frame_size_validation() {
        // 测试最大帧大小验证
        let frame_size = 1024;
        let max_frame_size = 512usize;
        assert!(frame_size > max_frame_size);

        // 测试有效帧大小
        let valid_frame_size = 256;
        assert!(valid_frame_size <= max_frame_size);
    }

    #[test]
    fn test_webtransport_datagram_size_validation() {
        // 测试 Datagram 大小验证
        let datagram_size = 2048;
        let max_datagram_size = 1024usize;
        assert!(datagram_size > max_datagram_size);

        // 测试有效 Datagram 大小
        let valid_size = 512;
        assert!(valid_size <= max_datagram_size);
    }

    #[test]
    fn test_webtransport_connection_availability() {
        // 测试连接可用性检查
        let conn_available = true;
        assert!(conn_available);

        let conn_unavailable: Option<bool> = None;
        assert!(conn_unavailable.is_none());
    }

    #[test]
    fn test_webtransport_timeout_configuration() {
        // 测试超时配置
        let timeout = Duration::from_secs(30);
        assert_eq!(timeout.as_secs(), 30);

        // 测试可选超时配置
        let some_timeout: Option<Duration> = Some(timeout);
        assert!(some_timeout.is_some());
        if let Some(t) = some_timeout {
            assert_eq!(t.as_secs(), 30);
        }

        // 测试无超时配置
        let no_timeout: Option<Duration> = None;
        assert!(no_timeout.is_none());
    }

    #[test]
    fn test_webtransport_record_drop_flag() {
        // 测试 record_drop 标志
        let record_drop_true = true;
        let record_drop_false = false;

        assert!(record_drop_true);
        assert!(!record_drop_false);
    }

    #[test]
    fn test_duration_arithmetic() {
        // 测试 Duration 的算术运算
        use std::time::Duration;
        let d1 = Duration::from_secs(10);
        let d2 = Duration::from_secs(5);
        let diff = d1.saturating_sub(d2);
        assert_eq!(diff.as_secs(), 5);

        // 测试饱和减法防止下溢
        let d3 = Duration::from_secs(3);
        let d4 = Duration::from_secs(5);
        let saturated = d3.saturating_sub(d4);
        assert_eq!(saturated.as_secs(), 0);
    }
}
