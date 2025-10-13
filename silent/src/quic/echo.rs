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
