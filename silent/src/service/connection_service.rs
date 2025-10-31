//! 与协议无关的连接处理抽象。
//!
//! `ConnectionService` trait 提供统一的接口来处理任意类型的网络连接，
//! 不依赖于具体的应用层协议（HTTP/gRPC/WebSocket 等）。
//!
//! # 核心概念
//!
//! - `BoxedConnection`: 类型擦除的连接流（可以是 TCP、Unix Socket、TLS 等）
//! - `BoxError`: 统一的错误类型
//! - `ConnectionFuture`: 异步连接处理的 Future
//!
//! # Examples
//!
//! 实现一个简单的 echo 服务：
//!
//! ```no_run
//! use silent::prelude::*;
//! use silent::service::connection_service::ConnectionService;
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! struct EchoService;
//!
//! impl ConnectionService for EchoService {
//!     fn call(&self, mut stream, peer) -> silent::service::connection_service::ConnectionFuture {
//!         Box::pin(async move {
//!             let mut buf = vec![0u8; 1024];
//!             loop {
//!                 let n = stream.read(&mut buf).await?;
//!                 if n == 0 {
//!                     break;
//!                 }
//!                 stream.write_all(&buf[..n]).await?;
//!             }
//!             Ok(())
//!         })
//!     }
//! }
//! ```
//!
//! 使用闭包（更简洁）：
//!
//! ```no_run
//! use silent::NetServer;
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! # async fn example() {
//! NetServer::new()
//!     .bind("127.0.0.1:8080".parse().unwrap())
//!     .serve(|mut stream, peer| async move {
//!         println!("Connection from: {}", peer);
//!         let mut buf = vec![0u8; 1024];
//!         let n = stream.read(&mut buf).await?;
//!         stream.write_all(&buf[..n]).await?;
//!         Ok(())
//!     })
//!     .await;
//! # }
//! ```

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::service::connection::BoxedConnection;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

/// 统一的错误类型，用于连接处理。
pub type BoxError = Box<dyn StdError + Send + Sync>;

/// 连接处理的 Future 类型。
pub type ConnectionFuture = Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send>>;

/// 与协议无关的连接处理服务。
///
/// 此 trait 定义了处理单个网络连接的统一接口，不依赖于具体的应用层协议。
///
/// # 实现方式
///
/// - **结构体实现**：适合复杂的状态管理和多个辅助方法
/// - **闭包实现**：自动通过 blanket impl 支持，适合简单场景
///
/// # Examples
///
/// 结构体实现：
///
/// ```no_run
/// use silent::service::connection_service::{ConnectionService, ConnectionFuture};
/// use silent::service::connection::BoxedConnection;
/// use silent::core::socket_addr::SocketAddr;
///
/// struct MyService {
///     config: String,
/// }
///
/// impl ConnectionService for MyService {
///     fn call(&self, stream: BoxedConnection, peer: SocketAddr) -> ConnectionFuture {
///         let config = self.config.clone();
///         Box::pin(async move {
///             // 使用 config 和 stream 处理连接
///             Ok(())
///         })
///     }
/// }
/// ```
///
/// 闭包实现（自动支持）：
///
/// ```no_run
/// use silent::NetServer;
///
/// # async fn example() {
/// NetServer::new()
///     .bind("127.0.0.1:8080".parse().unwrap())
///     .serve(|stream, peer| async move {
///         // 直接处理连接
///         Ok(())
///     })
///     .await;
/// # }
/// ```
pub trait ConnectionService: Send + Sync + 'static {
    /// 处理单个网络连接。
    ///
    /// # 参数
    ///
    /// - `stream`: 类型擦除的连接流（实现 `AsyncRead + AsyncWrite`）
    /// - `peer`: 对端的 socket 地址
    ///
    /// # 返回
    ///
    /// 返回一个 Future，成功时为 `Ok(())`，失败时包含错误信息。
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture;
}

/// 为闭包自动实现 `ConnectionService`。
///
/// 这允许直接使用闭包作为连接处理器，而无需手动实现 trait。
impl<F, Fut> ConnectionService for F
where
    F: Send + Sync + 'static + Fn(BoxedConnection, CoreSocketAddr) -> Fut,
    Fut: Future<Output = Result<(), BoxError>> + Send + 'static,
{
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        Box::pin((self)(stream, peer))
    }
}
