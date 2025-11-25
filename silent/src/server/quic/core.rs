use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use bytes::{Buf, Bytes};
use h3::server::RequestStream;
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
}

impl WebTransportStream {
    pub(crate) fn new(
        inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
        max_frame_size: Option<usize>,
        read_timeout: Option<Duration>,
    ) -> Self {
        Self {
            inner,
            max_frame_size,
            read_timeout,
        }
    }
    pub async fn recv_data(&mut self) -> Result<Option<Bytes>> {
        let fut = self.inner.recv_data();
        let maybe = match self.read_timeout {
            Some(t) => timeout(t, fut).await??,
            None => fut.await?,
        };
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
}
