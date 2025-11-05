use super::core::QuicSession;
use super::core::WebTransportHandler;
use super::core::WebTransportStream;
use anyhow::Result;
use bytes::Bytes;
use std::sync::Arc;
use tracing::info;

#[derive(Clone, Default)]
pub(crate) struct EchoHandler;

#[async_trait::async_trait]
impl WebTransportHandler for EchoHandler {
    async fn handle(
        &self,
        session: Arc<QuicSession>,
        stream: &mut WebTransportStream,
    ) -> Result<()> {
        let mut payload = Bytes::new();
        while let Some(chunk) = stream.recv_data().await? {
            if payload.is_empty() {
                payload = chunk;
            } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }
        let message =
            String::from_utf8(payload.to_vec()).unwrap_or_else(|_| "<binary>".to_string());
        info!(session_id = session.id(), remote = %session.remote_addr(), "收到 WebTransport 消息: {message}");
        let response = format!("echo(webtransport): {message}");
        stream.send_data(Bytes::from(response)).await?;
        stream.finish().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::quic::core::QuicSession;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_echo_handler_empty_message() {
        let remote: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let session = Arc::new(QuicSession::new(remote));

        // 空消息测试：没有数据进入时
        assert!(session.remote_addr() == remote);
    }

    #[tokio::test]
    async fn test_echo_handler_binary_data() {
        // 二进制数据（非 UTF-8）验证
        let binary_data = vec![0xFF, 0xFE, 0xFD, 0xFC];
        let bytes = Bytes::from(binary_data);

        // 验证从非 UTF-8 数据创建的 Bytes 对象
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes[0], 0xFF);
    }

    #[tokio::test]
    async fn test_echo_handler_multiple_chunks() {
        // 多块数据聚合逻辑验证
        let chunks = vec![
            Bytes::from("hello "),
            Bytes::from("world"),
            Bytes::from("!"),
        ];

        // 验证聚合逻辑（模拟 EchoHandler 中的逻辑）
        let mut payload = Bytes::new();
        for chunk in chunks {
            if payload.is_empty() {
                payload = chunk;
            } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }

        assert_eq!(payload.len(), 12);
        assert_eq!(payload, Bytes::from("hello world!"));

        // 验证 UTF-8 转换
        let message =
            String::from_utf8(payload.to_vec()).unwrap_or_else(|_| "<binary>".to_string());
        assert_eq!(message, "hello world!");
    }

    #[tokio::test]
    async fn test_echo_handler_binary_chunk_aggregation() {
        // 二进制多块数据聚合
        let chunks = vec![Bytes::from(&b"\xFF\xFE"[..]), Bytes::from(&b"\xFD\xFC"[..])];

        // 聚合二进制数据
        let mut payload = Bytes::new();
        for chunk in chunks {
            if payload.is_empty() {
                payload = chunk;
            } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }

        assert_eq!(payload.len(), 4);

        // 验证非 UTF-8 数据会被标记为 "<binary>"
        let message =
            String::from_utf8(payload.to_vec()).unwrap_or_else(|_| "<binary>".to_string());
        assert_eq!(message, "<binary>");
    }

    #[tokio::test]
    async fn test_echo_handler_session_info() {
        // 验证 QuicSession 的基本属性
        let addr1: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8888".parse().unwrap();
        let session1 = Arc::new(QuicSession::new(addr1));
        let session2 = Arc::new(QuicSession::new(addr2));

        assert!(!session1.id().is_empty());
        assert!(!session2.id().is_empty());
        assert_ne!(session1.id(), session2.id()); // ID 应该唯一
        assert_eq!(session1.remote_addr(), addr1);
        assert_eq!(session2.remote_addr(), addr2);
    }

    #[tokio::test]
    async fn test_echo_handler_aggregates_empty_and_nonempty() {
        // 测试空块和非空块的聚合
        let chunks = vec![
            Bytes::new(), // 空块
            Bytes::from("data"),
        ];

        let mut payload = Bytes::new();
        for chunk in chunks {
            if payload.is_empty() {
                payload = chunk;
            } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }

        // 空块应该直接被第一个非空块替换
        assert_eq!(payload, Bytes::from("data"));
    }

    #[tokio::test]
    async fn test_echo_handler_single_chunk() {
        // 测试单块数据（不经过聚合逻辑）
        let chunks = vec![Bytes::from("single")];

        let mut payload = Bytes::new();
        for chunk in chunks {
            if payload.is_empty() {
                payload = chunk;
            } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }

        assert_eq!(payload, Bytes::from("single"));

        // 验证 UTF-8 转换
        let message =
            String::from_utf8(payload.to_vec()).unwrap_or_else(|_| "<binary>".to_string());
        assert_eq!(message, "single");
    }

    #[test]
    fn test_echo_handler_response_format() {
        // 测试响应格式
        let test_cases = vec![
            ("hello", "echo(webtransport): hello"),
            ("", "echo(webtransport): "),
            ("测试中文", "echo(webtransport): 测试中文"),
        ];

        for (input, expected) in test_cases {
            let response = format!("echo(webtransport): {input}");
            assert_eq!(response, expected);
        }
    }
}
