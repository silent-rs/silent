//! RouteConnectionService 适配器
//!
//! 该模块提供 `RouteConnectionService` 适配器，将 `Route` 适配为 `ConnectionService`。
//! 这种设计解耦了路由逻辑与网络服务逻辑，使得 Route 可以专注于路由数据结构和处理，
//! 而网络连接处理通过适配器模式实现。

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::route::Route;
#[cfg(feature = "scheduler")]
use crate::scheduler::middleware::SchedulerMiddleware;
use crate::server::connection::BoxedConnection;
use crate::server::connection_service::{ConnectionFuture, ConnectionService};
use crate::server::protocol::hyper_http::HyperServiceHandler;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
#[cfg(feature = "quic")]
use std::sync::Arc;

/// RouteConnectionService 适配器
///
/// 将 `Route` 适配为 `ConnectionService`，负责处理网络连接并将其转发到路由系统。
///
/// # 设计理念
///
/// - **职责分离**: Route 专注于路由逻辑，RouteConnectionService 专注于连接处理
/// - **适配器模式**: 通过适配器解耦路由层与网络服务层
/// - **向后兼容**: Route 直接实现 ConnectionService 委托给此适配器
///
/// # 示例
///
/// ```rust,no_run
/// use silent::prelude::*;
/// let route = Route::new("").get(|_req: Request| async { Ok("hello world") });
/// // 直接使用 Route（推荐）
/// Server::new().run(route);
/// ```
#[derive(Clone)]
pub struct RouteConnectionService {
    route: Route,
}

impl RouteConnectionService {
    /// 创建新的 RouteConnectionService 实例
    #[inline]
    pub fn new(route: Route) -> Self {
        Self { route }
    }

    /// 获取内部路由的引用
    #[inline]
    #[allow(dead_code)]
    pub fn route(&self) -> &Route {
        &self.route
    }

    /// 处理 HTTP 连接（HTTP/1.1 或 HTTP/2）
    ///
    /// 直接使用 hyper 的 auto builder 处理连接，无需额外的 Serve 中间层。
    fn handle_http_connection(
        root_route: Route,
        stream: BoxedConnection,
        peer: CoreSocketAddr,
    ) -> ConnectionFuture {
        #[allow(unused_mut)]
        let mut root_route = root_route;
        #[cfg(feature = "session")]
        root_route.check_session();
        #[cfg(feature = "cookie")]
        root_route.check_cookie();
        #[cfg(feature = "scheduler")]
        root_route.hook_first(SchedulerMiddleware::new());

        let routes = root_route.convert_to_route_tree();
        Box::pin(async move {
            let io = TokioIo::new(stream);
            let builder = Builder::new(TokioExecutor::new());
            builder
                .serve_connection_with_upgrades(io, HyperServiceHandler::new(peer, routes))
                .await
        })
    }
}

impl ConnectionService for RouteConnectionService {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        // 尝试将连接转换为 QuicConnection
        #[cfg(feature = "quic")]
        {
            use crate::quic::connection::QuicConnection;
            match stream.downcast::<QuicConnection>() {
                Ok(quic) => {
                    // QUIC 连接处理
                    let routes = Arc::new(self.route.clone());
                    Box::pin(async move {
                        let incoming = quic.into_incoming();
                        crate::quic::service::handle_quic_connection(incoming, routes)
                            .await
                            .map_err(Into::into)
                    })
                }
                Err(stream) => {
                    // 不是 QUIC 连接，继续处理为 HTTP/1.1 或 HTTP/2
                    Self::handle_http_connection(self.route.clone(), stream, peer)
                }
            }
        }

        // 没有 QUIC feature 时的 HTTP/1.1 或 HTTP/2 连接处理
        #[cfg(not(feature = "quic"))]
        Self::handle_http_connection(self.route.clone(), stream, peer)
    }
}

/// 从 Route 自动转换为 RouteConnectionService
///
/// 这个实现提供内部转换能力，但通常不需要显式使用，
/// 因为 Route 直接实现了 ConnectionService 并委托给此适配器。
impl From<Route> for RouteConnectionService {
    #[inline]
    fn from(route: Route) -> Self {
        Self::new(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_connection_service_creation() {
        let route = Route::new("");
        let service = RouteConnectionService::new(route.clone());
        assert_eq!(service.route().path, route.path);
    }

    #[test]
    fn test_from_trait() {
        let route = Route::new("test");
        let service = RouteConnectionService::from(route.clone());
        assert_eq!(service.route().path, route.path);
    }
}
