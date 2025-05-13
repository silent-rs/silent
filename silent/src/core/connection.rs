use tokio::io::{AsyncRead, AsyncWrite};

pub trait Connection: AsyncRead + AsyncWrite + Unpin {}

impl<S> Connection for S where S: AsyncRead + AsyncWrite + Unpin {}
