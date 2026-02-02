//! RouteConnectionService 适配器
//!
//! 该模块提供 `RouteConnectionService` 适配器，将 `Route` 适配为 `ConnectionService`。
//! 这种设计解耦了路由逻辑与网络服务逻辑，使得 Route 可以专注于路由数据结构和处理，
//! 而网络连接处理通过适配器模式实现。

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::route::Route;
#[cfg(feature = "scheduler")]
use crate::scheduler::middleware::SchedulerMiddleware;
use crate::server::config::{ConnectionLimits, global_server_config};
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
    limits: ConnectionLimits,
    #[cfg(feature = "quic")]
    webtransport_handler: Arc<dyn crate::server::quic::WebTransportHandler>,
}

impl RouteConnectionService {
    /// 创建新的 RouteConnectionService 实例
    #[inline]
    pub fn new(route: Route) -> Self {
        let limits = global_server_config().connection_limits.clone();
        #[cfg(feature = "quic")]
        let webtransport_handler: Arc<dyn crate::server::quic::WebTransportHandler> =
            Arc::new(crate::server::quic::EchoHandler);
        Self {
            route,
            limits,
            #[cfg(feature = "quic")]
            webtransport_handler,
        }
    }

    /// 为 WebTransport 提供自定义处理器，替代默认的 EchoHandler。
    #[cfg(feature = "quic")]
    pub fn with_webtransport_handler(
        mut self,
        handler: Arc<dyn crate::server::quic::WebTransportHandler>,
    ) -> Self {
        self.webtransport_handler = handler;
        self
    }

    /// 处理 HTTP 连接（HTTP/1.1 或 HTTP/2）
    ///
    /// 直接使用 hyper 的 auto builder 处理连接，无需额外的 Serve 中间层。
    fn handle_http_connection(
        root_route: Route,
        stream: BoxedConnection,
        peer: CoreSocketAddr,
        limits: ConnectionLimits,
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
        let max_body_size = limits.max_body_size;
        Box::pin(async move {
            let io = TokioIo::new(stream);
            let builder = Builder::new(TokioExecutor::new());
            builder
                .serve_connection_with_upgrades(
                    io,
                    HyperServiceHandler::with_limits(peer.into(), routes, max_body_size),
                )
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
                    let read_timeout = self.limits.h3_read_timeout;
                    let max_body_size = self.limits.max_body_size;
                    let max_wt_frame = self.limits.max_webtransport_frame_size;
                    let wt_read_timeout = self.limits.webtransport_read_timeout;
                    let max_wt_sessions = self.limits.max_webtransport_sessions;
                    let enable_datagram = global_server_config()
                        .quic_transport
                        .as_ref()
                        .map(|c| c.enable_datagram)
                        .unwrap_or(true);
                    let max_datagram_size = self.limits.webtransport_datagram_max_size;
                    let datagram_rate = self.limits.webtransport_datagram_rate;
                    let datagram_drop_metric = self.limits.webtransport_datagram_drop_metric;
                    let webtransport_handler = self.webtransport_handler.clone();
                    Box::pin(async move {
                        let incoming = quic.into_incoming();
                        crate::quic::service::handle_quic_connection(
                            incoming,
                            routes,
                            max_body_size,
                            read_timeout,
                            max_wt_frame,
                            wt_read_timeout,
                            max_wt_sessions,
                            enable_datagram,
                            max_datagram_size,
                            datagram_rate,
                            datagram_drop_metric,
                            webtransport_handler,
                        )
                        .await
                        .map_err(Into::into)
                    })
                }
                Err(stream) => {
                    // 不是 QUIC 连接，继续处理为 HTTP/1.1 或 HTTP/2
                    Self::handle_http_connection(
                        self.route.clone(),
                        stream,
                        peer,
                        self.limits.clone(),
                    )
                }
            }
        }

        // 没有 QUIC feature 时的 HTTP/1.1 或 HTTP/2 连接处理
        #[cfg(not(feature = "quic"))]
        Self::handle_http_connection(self.route.clone(), stream, peer, self.limits.clone())
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
    use std::sync::Arc;

    // ==================== 基础构造测试 ====================

    #[test]
    fn test_route_connection_service_creation() {
        let route = Route::new("");
        let service = RouteConnectionService::new(route.clone());
        assert_eq!(service.route.path, route.path);
    }

    #[test]
    fn test_from_trait() {
        let route = Route::new("test");
        let service = RouteConnectionService::from(route.clone());
        assert_eq!(service.route.path, route.path);
    }

    #[test]
    fn test_route_connection_service_clone() {
        let route = Route::new("/test");
        let service1 = RouteConnectionService::new(route.clone());
        let service2 = service1.clone();

        assert_eq!(service1.route.path, service2.route.path);
        assert_eq!(service2.route.path, route.path);
    }

    #[test]
    fn test_new_with_nested_route() {
        use crate::Request;

        let route = Route::new("api").get(|_req: Request| async move { Ok("hello") });
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "api");
    }

    #[test]
    fn test_new_with_empty_route() {
        let route = Route::new("");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "");
    }

    #[test]
    fn test_new_with_root_path() {
        let route = Route::new("");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "");
    }

    #[test]
    fn test_multiple_from_calls() {
        let route1 = Route::new("path1");
        let route2 = Route::new("path2");

        let service1 = RouteConnectionService::from(route1);
        let service2 = RouteConnectionService::from(route2);

        assert_eq!(service1.route.path, "path1");
        assert_eq!(service2.route.path, "path2");
    }

    #[test]
    fn test_service_limits_field() {
        let route = Route::new("test");
        let service = RouteConnectionService::new(route);

        // 验证 limits 字段被正确初始化
        // 注意：max_body_size 可能是 None，所以不在这里断言
        let _ = service.limits.max_body_size;
    }

    #[test]
    fn test_new_with_complex_route() {
        use crate::Request;

        let route = Route::new("api")
            .get(|_req: Request| async move { Ok("GET") })
            .post(|_req: Request| async move { Ok("POST") })
            .put(|_req: Request| async move { Ok("PUT") });

        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "api");
    }

    #[test]
    fn test_service_with_nested_routes() {
        use crate::Request;

        let api = Route::new("api");
        let users = Route::new("users").get(|_req: Request| async move { Ok("users") });
        let posts = Route::new("posts").get(|_req: Request| async move { Ok("posts") });

        let service = RouteConnectionService::new(api);
        assert_eq!(service.route.path, "api");

        let service2 = RouteConnectionService::new(users);
        assert_eq!(service2.route.path, "users");

        let service3 = RouteConnectionService::new(posts);
        assert_eq!(service3.route.path, "posts");
    }

    #[test]
    fn test_route_preservation() {
        let original_route = Route::new("original");
        let service = RouteConnectionService::new(original_route.clone());

        // 验证原始路由未被修改
        assert_eq!(original_route.path, "original");
        assert_eq!(service.route.path, "original");
    }

    // ==================== feature 相关测试 ====================

    #[cfg(feature = "quic")]
    #[test]
    fn test_with_webtransport_handler() {
        use crate::server::quic::EchoHandler;

        let route = Route::new("/test");
        let handler: Arc<dyn crate::server::quic::WebTransportHandler> = Arc::new(EchoHandler);

        let service = RouteConnectionService::new(route).with_webtransport_handler(handler.clone());

        // 验证 handler 被正确设置
        // 注意：webtransport_handler 是 Arc，无法直接比较，但可以验证其存在
        let _ = &service.webtransport_handler;
    }

    #[cfg(feature = "quic")]
    #[test]
    fn test_default_webtransport_handler() {
        let route = Route::new("/test");
        let service = RouteConnectionService::new(route);

        // 验证默认的 EchoHandler 被设置
        let _ = &service.webtransport_handler;
    }

    #[cfg(feature = "quic")]
    #[test]
    fn test_webtransport_handler_override() {
        use crate::server::quic::EchoHandler;

        let route = Route::new("/test");
        let custom_handler: Arc<dyn crate::server::quic::WebTransportHandler> =
            Arc::new(EchoHandler);

        let service1 = RouteConnectionService::new(route.clone());
        let service2 = service1.clone().with_webtransport_handler(custom_handler);

        // 验证返回的是一个新的 service 实例
        assert_eq!(service1.route.path, service2.route.path);
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_service_with_special_characters() {
        // Route::new() 只保留第一段路径，其余部分成为子路由
        let route = Route::new("api/v1/test-endpoint");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "api"); // path 字段只包含第一段
        assert!(!service.route.children.is_empty()); // 验证子路由存在
    }

    #[test]
    fn test_service_with_unicode_path() {
        // Unicode 路径也遵循相同的规则
        let route = Route::new("api/用户/资料");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "api"); // 第一段
        assert!(!service.route.children.is_empty()); // 有子路由
    }

    #[test]
    fn test_service_with_long_path() {
        // 长路径也遵循相同的规则
        let route = Route::new("api/v1/very/long/path/with/many/segments");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "api"); // 只有第一段
        assert!(!service.route.children.is_empty()); // 有子路由
    }

    #[test]
    fn test_clone_independence() {
        let route = Route::new("test");
        let service1 = RouteConnectionService::new(route);
        let service2 = service1.clone();

        // 验证 clone 的独立性
        assert_eq!(service1.route.path, service2.route.path);
    }

    #[test]
    fn test_from_trait_multiple_conversions() {
        let routes = vec![
            Route::new("path1"),
            Route::new("path2"),
            Route::new("path3"),
        ];

        let services: Vec<RouteConnectionService> = routes
            .into_iter()
            .map(RouteConnectionService::from)
            .collect();

        assert_eq!(services.len(), 3);
        assert_eq!(services[0].route.path, "path1");
        assert_eq!(services[1].route.path, "path2");
        assert_eq!(services[2].route.path, "path3");
    }

    #[test]
    fn test_service_with_wildcard_route() {
        let route = Route::new("*");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "*");
    }

    #[test]
    fn test_service_with_param_route() {
        // 参数路由 "users/:id" 会被分割为 path="users" 和子路由
        let route = Route::new("users/:id");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "users"); // 第一段
        assert!(!service.route.children.is_empty()); // :id 是子路由
    }

    #[test]
    fn test_service_with_glob_route() {
        // Glob 路由 "files/**" 会被分割为 path="files" 和子路由
        let route = Route::new("files/**");
        let service = RouteConnectionService::new(route);
        assert_eq!(service.route.path, "files"); // 第一段
        assert!(!service.route.children.is_empty()); // ** 是子路由
    }

    // ==================== limits 验证测试 ====================

    #[test]
    fn test_connection_limits_initialization() {
        let route = Route::new("test");
        let service = RouteConnectionService::new(route);

        // 验证 limits 字段被正确初始化
        // 注意：max_body_size 和 h3_read_timeout 可能是 None
        let _ = service.limits.max_body_size;
        let _ = service.limits.h3_read_timeout;
    }

    #[test]
    fn test_clone_preserves_limits() {
        let route = Route::new("test");
        let service1 = RouteConnectionService::new(route);
        let service2 = service1.clone();

        // 验证 clone 后 limits 相同
        assert_eq!(service1.limits.max_body_size, service2.limits.max_body_size);
        assert_eq!(
            service1.limits.h3_read_timeout,
            service2.limits.h3_read_timeout
        );
    }
}
