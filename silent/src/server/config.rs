use std::sync::{RwLock, RwLockReadGuard};
use std::time::Duration;

/// 连接级别的保护配置。
#[derive(Clone, Debug, Default)]
pub struct ConnectionLimits {
    /// 处理单个连接（含 HTTP1/2/3）的超时时间，超时后任务将被取消。
    pub handler_timeout: Option<Duration>,
    /// HTTP 请求体大小上限（字节）。`None` 表示不限制。
    pub max_body_size: Option<usize>,
    /// QUIC/HTTP3 请求体读取超时。
    pub h3_read_timeout: Option<Duration>,
    /// WebTransport 单帧/消息大小上限（字节）。
    pub max_webtransport_frame_size: Option<usize>,
    /// WebTransport 读取超时。
    pub webtransport_read_timeout: Option<Duration>,
    /// WebTransport 会话并发上限。
    pub max_webtransport_sessions: Option<usize>,
}

/// Server 级配置入口。
#[derive(Clone, Debug, Default)]
pub struct ServerConfig {
    pub connection_limits: ConnectionLimits,
    /// QUIC 传输参数（仅在 `quic` 特性开启时生效）。
    #[cfg(feature = "quic")]
    pub quic_transport: Option<crate::server::quic::QuicTransportConfig>,
}

/// 运行时可查询的配置注册表，便于 RouteConnectionService 获取 Server 配置。
///
/// 注意：不是全局单例配置源，只用于当前进程内 server 启动时的传递。
#[derive(Default)]
struct ServerConfigRegistry {
    inner: RwLock<ServerConfig>,
}

static CONFIG_REGISTRY: ServerConfigRegistry = ServerConfigRegistry {
    inner: RwLock::new(ServerConfig {
        connection_limits: ConnectionLimits {
            handler_timeout: None,
            max_body_size: None,
            h3_read_timeout: None,
            max_webtransport_frame_size: None,
            webtransport_read_timeout: None,
            max_webtransport_sessions: None,
        },
        #[cfg(feature = "quic")]
        quic_transport: None,
    }),
};

impl ServerConfigRegistry {
    pub fn set(config: ServerConfig) {
        if let Ok(mut guard) = CONFIG_REGISTRY.inner.write() {
            *guard = config;
        }
    }

    pub fn get() -> RwLockReadGuard<'static, ServerConfig> {
        CONFIG_REGISTRY
            .inner
            .read()
            .expect("server config registry poisoned")
    }
}

pub fn set_global_server_config(config: ServerConfig) {
    ServerConfigRegistry::set(config);
}

pub fn global_server_config() -> RwLockReadGuard<'static, ServerConfig> {
    ServerConfigRegistry::get()
}
