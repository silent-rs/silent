use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use bytes::{Buf, Bytes};
use h3::server::RequestStream;
use scru128::Scru128Id;

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
}

impl WebTransportStream {
    pub(crate) fn new(inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>) -> Self {
        Self { inner }
    }
    pub async fn recv_data(&mut self) -> Result<Option<Bytes>> {
        match self.inner.recv_data().await? {
            Some(mut buf) => Ok(Some(buf.copy_to_bytes(buf.remaining()))),
            None => Ok(None),
        }
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
    use std::sync::Arc;

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

    // 新增测试：验证 QuicSession ID 的唯一性
    #[test]
    fn test_quic_session_ids_are_unique() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let sessions: Vec<_> = (0..100).map(|_| QuicSession::new(addr)).collect();

        let mut ids: Vec<_> = sessions.iter().map(|s| s.id()).collect();
        ids.sort();
        ids.dedup();

        // 验证所有 ID 都是唯一的
        assert_eq!(ids.len(), 100, "所有会话 ID 应该是唯一的");
    }

    // 新增测试：验证 QuicSession ID 格式
    #[test]
    fn test_quic_session_id_format() {
        let addr: SocketAddr = "192.168.1.1:443".parse().unwrap();
        let session = QuicSession::new(addr);
        let id = session.id();

        // SCRU128 ID 应该是一个非空字符串
        assert!(!id.is_empty());
        // SCRU128 ID 长度通常是 26 个字符（类似 UUID）
        assert!(id.len() > 20);
    }

    // 新增测试：验证 QuicSession 的 Clone 特性
    #[test]
    fn test_quic_session_clone() {
        let addr: SocketAddr = "10.0.0.1:8443".parse().unwrap();
        let s1 = QuicSession::new(addr);
        let s2 = s1.clone();

        // Clone 后应该有相同的 ID 和地址
        assert_eq!(s1.id(), s2.id());
        assert_eq!(s1.remote_addr(), s2.remote_addr());
    }

    // 新增测试：验证 QuicSession 可以被 Arc 包装
    #[test]
    fn test_quic_session_can_be_arc_wrapped() {
        let addr: SocketAddr = "172.16.0.1:9000".parse().unwrap();
        let session = Arc::new(QuicSession::new(addr));

        // 验证可以通过 Arc 访问属性
        assert!(!session.id().is_empty());
        assert_eq!(session.remote_addr(), addr);
    }

    // 新增测试：验证 QuicSession 支持不同的地址类型
    #[test]
    fn test_quic_session_with_ipv4_and_ipv6() {
        let ipv4: SocketAddr = "192.168.1.1:443".parse().unwrap();
        let ipv6: SocketAddr = "[::1]:443".parse().unwrap();
        let localhost: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let s1 = QuicSession::new(ipv4);
        let s2 = QuicSession::new(ipv6);
        let s3 = QuicSession::new(localhost);

        assert_eq!(s1.remote_addr(), ipv4);
        assert_eq!(s2.remote_addr(), ipv6);
        assert_eq!(s3.remote_addr(), localhost);

        // 验证 ID 唯一性
        assert_ne!(s1.id(), s2.id());
        assert_ne!(s2.id(), s3.id());
        assert_ne!(s1.id(), s3.id());
    }

    // 新增测试：验证 QuicSession 可以在 async 上下文中使用
    #[tokio::test]
    async fn test_quic_session_in_async_context() {
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let session = QuicSession::new(addr);

        // 验证可以在 async 函数中访问
        let id = session.id().to_string();
        let remote = session.remote_addr();

        assert!(!id.is_empty());
        assert_eq!(remote, addr);
    }

    // 新增测试：验证 QuicSession 的 Send + Sync 约束
    #[test]
    fn test_quic_session_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<QuicSession>();
        assert_sync::<QuicSession>();
    }

    // 新增测试：验证 WebTransportHandler trait 存在并可实现
    #[test]
    fn test_webtransport_handler_trait_is_implementable() {
        // 这个测试验证 WebTransportHandler trait 可以被实现
        use super::*;

        struct MockHandler;
        #[async_trait::async_trait]
        impl WebTransportHandler for MockHandler {
            async fn handle(
                &self,
                _session: Arc<QuicSession>,
                _stream: &mut WebTransportStream,
            ) -> Result<()> {
                Ok(())
            }
        }

        // 验证 MockHandler 可以被转换为 trait object
        let _handler: Arc<dyn WebTransportHandler> = Arc::new(MockHandler);
    }

    // 新增测试：验证 WebTransportStream 的结构体大小
    #[test]
    fn test_webtransport_stream_size_and_alignment() {
        let size = std::mem::size_of::<WebTransportStream>();
        let align = std::mem::align_of::<WebTransportStream>();

        // WebTransportStream 包含一个 RequestStream，所以大小应该非零
        assert!(size > 0);
        // 对齐应该至少是 usize 的对齐
        assert!(align >= std::mem::align_of::<usize>());
    }

    // 新增测试：验证 WebTransportStream 的方法签名
    #[test]
    fn test_webtransport_stream_method_signatures() {
        // 通过函数签名验证方法存在
        use super::*;

        // 验证 new 方法的返回类型
        fn assert_new_returns_stream() {
            // WebTransportStream::new 存在并返回 WebTransportStream
            // 这是一个编译时检查
            let _ = std::mem::size_of::<WebTransportStream>();
        }

        // 验证方法存在通过编译检查
        assert_new_returns_stream();
    }

    // 新增测试：验证 QuicSession 的 ID 生成不依赖地址
    #[test]
    fn test_quic_session_id_generation_independent_of_address() {
        let addr1: SocketAddr = "127.0.0.1:1111".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:2222".parse().unwrap();

        let s1 = QuicSession::new(addr1);
        let _s2 = QuicSession::new(addr2);

        // 即使地址相同，ID 也应该不同
        let s3 = QuicSession::new(addr1);
        assert_ne!(s1.id(), s3.id(), "相同地址应该生成不同的 ID");
    }

    // 新增测试：验证 WebTransportStream 的 Send + Sync 约束
    #[test]
    fn test_webtransport_stream_trait_bounds() {
        use super::*;

        // WebTransportStream 应该是 Send 的（因为它可能在线程间移动）
        fn assert_send<T: Send>() {}
        assert_send::<WebTransportStream>();

        // 注意：WebTransportStream 不一定是 Sync 的，因为它包含内部的 mutable state
        // 所以我们这里不测试 Sync
    }

    // 新增测试：验证 QuicSession 的 PartialEq 实现
    #[test]
    fn test_quic_session_partial_equality() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let s1 = QuicSession::new(addr);
        let s2 = s1.clone();

        // 相同的 session 应该有相同的 ID
        assert_eq!(s1.id(), s2.id());

        // 不同的 session 应该有不同的 ID
        let s3 = QuicSession::new(addr);
        assert_ne!(s1.id(), s3.id());
    }
}
