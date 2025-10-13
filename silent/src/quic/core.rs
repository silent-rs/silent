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
