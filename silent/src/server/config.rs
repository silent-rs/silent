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
    /// WebTransport 每帧大小上限（datagram/stream 共享）；None 表示不限制。
    pub webtransport_datagram_max_size: Option<usize>,
    /// WebTransport 每连接 datagram 速率（每秒）上限。
    pub webtransport_datagram_rate: Option<u64>,
    /// WebTransport datagram 丢弃计数（只做观测）。
    pub webtransport_datagram_drop_metric: bool,
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
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_limits_default() {
        let limits = ConnectionLimits::default();
        assert_eq!(limits.handler_timeout, None);
        assert_eq!(limits.max_body_size, None);
        assert_eq!(limits.h3_read_timeout, None);
        assert_eq!(limits.max_webtransport_frame_size, None);
        assert_eq!(limits.webtransport_read_timeout, None);
        assert_eq!(limits.max_webtransport_sessions, None);
        assert_eq!(limits.webtransport_datagram_max_size, None);
        assert_eq!(limits.webtransport_datagram_rate, None);
        assert!(!limits.webtransport_datagram_drop_metric);
    }

    #[test]
    fn test_connection_limits_clone() {
        let limits = ConnectionLimits {
            handler_timeout: Some(std::time::Duration::from_secs(30)),
            max_body_size: Some(1024),
            h3_read_timeout: Some(std::time::Duration::from_secs(20)),
            max_webtransport_frame_size: Some(4096),
            webtransport_read_timeout: Some(std::time::Duration::from_secs(10)),
            max_webtransport_sessions: Some(100),
            webtransport_datagram_max_size: Some(1350),
            webtransport_datagram_rate: Some(1000),
            webtransport_datagram_drop_metric: true,
        };

        let cloned = limits.clone();
        assert_eq!(cloned.handler_timeout, limits.handler_timeout);
        assert_eq!(cloned.max_body_size, limits.max_body_size);
        assert_eq!(cloned.h3_read_timeout, limits.h3_read_timeout);
        assert_eq!(
            cloned.max_webtransport_frame_size,
            limits.max_webtransport_frame_size
        );
        assert_eq!(
            cloned.webtransport_read_timeout,
            limits.webtransport_read_timeout
        );
        assert_eq!(
            cloned.max_webtransport_sessions,
            limits.max_webtransport_sessions
        );
        assert_eq!(
            cloned.webtransport_datagram_max_size,
            limits.webtransport_datagram_max_size
        );
        assert_eq!(
            cloned.webtransport_datagram_rate,
            limits.webtransport_datagram_rate
        );
        assert_eq!(
            cloned.webtransport_datagram_drop_metric,
            limits.webtransport_datagram_drop_metric
        );
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.connection_limits.handler_timeout, None);
        assert_eq!(config.connection_limits.max_body_size, None);
    }

    #[test]
    fn test_server_config_clone() {
        let config = ServerConfig {
            connection_limits: ConnectionLimits {
                handler_timeout: Some(std::time::Duration::from_secs(60)),
                ..Default::default()
            },
            ..Default::default()
        };

        let cloned = config.clone();
        assert_eq!(
            cloned.connection_limits.handler_timeout,
            config.connection_limits.handler_timeout
        );
    }

    #[test]
    fn test_set_global_server_config() {
        let custom_config = ServerConfig {
            connection_limits: ConnectionLimits {
                handler_timeout: Some(std::time::Duration::from_secs(120)),
                max_body_size: Some(2048),
                ..Default::default()
            },
            ..Default::default()
        };

        set_global_server_config(custom_config);

        let config = global_server_config();
        assert_eq!(
            config.connection_limits.handler_timeout,
            Some(std::time::Duration::from_secs(120))
        );
        assert_eq!(config.connection_limits.max_body_size, Some(2048));
    }

    #[test]
    fn test_global_server_config_multiple_reads() {
        set_global_server_config(ServerConfig::default());

        // 可以多次读取全局配置
        let config1 = global_server_config();
        let config2 = global_server_config();

        assert_eq!(
            config1.connection_limits.handler_timeout,
            config2.connection_limits.handler_timeout
        );
    }

    #[test]
    fn test_connection_limits_debug() {
        let limits = ConnectionLimits {
            handler_timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        };

        // 验证 Debug trait 实现
        let debug_str = format!("{:?}", limits);
        assert!(debug_str.contains("ConnectionLimits"));
    }

    #[test]
    fn test_server_config_debug() {
        let config = ServerConfig::default();

        // 验证 Debug trait 实现
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ServerConfig"));
    }
}
