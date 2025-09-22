use tokio::io::{AsyncRead, AsyncWrite};
// TODO(runtime): 未来可考虑为非 tokio I/O 实现内部适配层，
// 但对外不暴露具体运行时 I/O trait，保持 Connection 作为公共边界。

pub trait Connection: AsyncRead + AsyncWrite + Unpin {}

impl<S> Connection for S where S: AsyncRead + AsyncWrite + Unpin {}
