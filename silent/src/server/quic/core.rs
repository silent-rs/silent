use std::{net::SocketAddr, sync::Arc, time::Duration, time::Instant};

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
    max_datagram_size: Option<usize>,
    datagram_per_sec: Option<u64>,
    datagram_tokens: u64,
    last_refill: Instant,
    record_drop: bool,
}

impl WebTransportStream {
    pub(crate) fn new(
        inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
        max_frame_size: Option<usize>,
        read_timeout: Option<Duration>,
        max_datagram_size: Option<usize>,
        datagram_per_sec: Option<u64>,
        record_drop: bool,
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
    #[allow(dead_code)]
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
        // h3 RequestStream 不直接暴露 datagram 发送；预留接口，后续可接入底层通道。
        anyhow::bail!("Datagram send not supported in this adapter yet");
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
