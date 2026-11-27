//! 日志初始化：tracing 输出到标准输出，格式和过滤规则来自 `[log]`。

use crate::config::LogFormat;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::AppConfig;

/// 初始化日志。已经初始化过（例如测试框架或宿主程序先设了 subscriber）就什么都不做。
///
/// - 过滤规则就是 `tracing_subscriber` 的 `EnvFilter` 语法：`"info"`、`"debug,sea_orm=warn"`；
/// - 时间用本地时区的 RFC 3339；
/// - JSON 格式把事件的字段摊平到顶层，`message`、`trace_id` 这些键一眼就能看到。
pub fn init(config: &AppConfig) -> Result<(), LogError> {
    let (level, format) = config.log_settings();
    let filter =
        tracing_subscriber::EnvFilter::try_new(&level).map_err(|source| LogError::Filter {
            level: level.clone(),
            source,
        })?;

    let subscriber = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => {
            let layer = tracing_subscriber::fmt::layer()
                .with_timer(ChronoLocal::rfc_3339())
                .with_target(false);
            if subscriber.with(layer).try_init().is_err() {
                // 已经设置过全局 subscriber。
            }
        }
        LogFormat::Json => {
            let layer = tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .with_timer(ChronoLocal::rfc_3339())
                .with_target(true)
                .with_file(false)
                .with_line_number(false);
            if subscriber.with(layer).try_init().is_err() {
                // 已经设置过全局 subscriber。
            }
        }
    }
    Ok(())
}

/// 日志初始化失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// 过滤规则写错了。
    #[error("log.level {level:?} 不是合法的过滤规则：{source}")]
    Filter {
        /// 配置里写的规则。
        level: String,
        /// 解析器给出的错误。
        source: tracing_subscriber::filter::ParseError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_filter_directives_are_reported() {
        let config: AppConfig = toml::from_str("[log]\nlevel = \"info==\"\n").unwrap();
        let err = init(&config).unwrap_err();
        assert!(matches!(err, LogError::Filter { .. }), "{err}");
        assert!(err.to_string().contains("info=="), "{err}");
    }

    #[test]
    fn init_is_idempotent() {
        let config = AppConfig::default();
        init(&config).unwrap();
        init(&config).unwrap();
    }
}
