//! 建立数据库连接：把配置里的 `[database]` 变成一个连接池。
//!
//! 用哪种数据库由 cargo feature 决定：`sqlite`（默认）、`mysql`、`postgres`。
//! 地址里的协议和编译时启用的 feature 对不上时，[`crate::config::AppConfig::validate`] 会先报错。

use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sea_orm::{ConnectOptions as SeaConnectOptions, Database, DatabaseConnection, DbErr};

/// 连接参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOptions<'a> {
    /// 数据库地址：`sqlite://…`、`mysql://…`、`postgres://…`。
    pub url: &'a str,
    /// 连接池上限。
    pub max_connections: u32,
    /// 连接池保持的最少连接数。
    pub min_connections: u32,
    /// 建立连接的时限。
    pub connect_timeout: Duration,
    /// 是否把每条 SQL 写进日志（`debug` 级别）。开发时开着，线上关掉。
    pub sql_logging: bool,
}

impl<'a> ConnectOptions<'a> {
    /// 只给地址，其余用默认值：上限 16 个连接，8 秒时限，记录 SQL。
    pub fn new(url: &'a str) -> Self {
        Self {
            url,
            max_connections: 16,
            min_connections: 0,
            connect_timeout: Duration::from_secs(8),
            sql_logging: true,
        }
    }

    /// 从配置的 `[database]` 取参数。`production` 环境不记录 SQL。
    pub fn from_config(config: &'a crate::config::DatabaseConfig, production: bool) -> Self {
        Self {
            url: &config.url,
            max_connections: config.max_connections,
            min_connections: config.min_connections,
            connect_timeout: config.connect_timeout.into(),
            sql_logging: !production,
        }
    }
}

/// 连不上数据库的原因。
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// SQLite 的文件所在的目录建不出来。
    #[error("创建数据库文件所在的目录 {} 失败：{source}", path.display())]
    CreateDir {
        /// 建不出来的目录。
        path: PathBuf,
        /// 文件系统的错误。
        source: io::Error,
    },
    /// 数据库返回的错误。
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// 按参数建立连接池。
///
/// SQLite 的地址指向文件时，会先建好文件所在的目录，并在地址里补上 `mode=rwc`，
/// 这样第一次运行就能直接建库。
pub async fn connect(options: &ConnectOptions<'_>) -> Result<DatabaseConnection, ConnectError> {
    ensure_sqlite_dir(options.url)?;
    let url = prepare_url(options.url);
    let mut sea = SeaConnectOptions::new(url.as_ref());
    if options.max_connections > 0 {
        sea.max_connections(options.max_connections);
    }
    sea.min_connections(options.min_connections);
    sea.acquire_timeout(options.connect_timeout);
    // SQL 记在 debug 级别：`log.level = "info"` 时每个请求只有一行访问日志，调成 debug 才看到每条语句。
    sea.sqlx_logging(options.sql_logging);
    if options.sql_logging {
        sea.sqlx_logging_level(log::LevelFilter::Debug);
    }
    Ok(Database::connect(sea).await?)
}

/// SQLite 的地址指向文件时，先建好文件所在的目录。
fn ensure_sqlite_dir(url: &str) -> Result<(), ConnectError> {
    let Some(rest) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
    else {
        return Ok(());
    };
    let (file, _) = rest.split_once('?').unwrap_or((rest, ""));
    if file.is_empty() || file == ":memory:" {
        return Ok(());
    }
    if let Some(parent) = Path::new(file).parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|source| ConnectError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// SQLite 的文件地址补上 `mode=rwc`，第一次运行就能直接建库。其余地址原样返回。
fn prepare_url(url: &str) -> Cow<'_, str> {
    let Some(rest) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
    else {
        return url.into();
    };
    let (file, query) = rest.split_once('?').unwrap_or((rest, ""));
    if file.is_empty()
        || file == ":memory:"
        || query.split('&').any(|pair| pair.starts_with("mode="))
    {
        return url.into();
    }
    let separator = if query.is_empty() { '?' } else { '&' };
    format!("{url}{separator}mode=rwc").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_file_urls_get_a_writable_mode() {
        assert_eq!(
            prepare_url("sqlite://data/app.db"),
            "sqlite://data/app.db?mode=rwc"
        );
        assert_eq!(
            prepare_url("sqlite://data/app.db?cache=shared"),
            "sqlite://data/app.db?cache=shared&mode=rwc"
        );
        assert_eq!(
            prepare_url("sqlite://data/app.db?mode=ro"),
            "sqlite://data/app.db?mode=ro"
        );
        assert_eq!(prepare_url("sqlite::memory:"), "sqlite::memory:");
        assert_eq!(prepare_url("sqlite://:memory:"), "sqlite://:memory:");
    }

    #[test]
    fn other_urls_pass_through() {
        assert_eq!(prepare_url("mysql://u:p@h/db"), "mysql://u:p@h/db");
    }

    #[tokio::test]
    async fn sqlite_parent_directory_is_created_and_database_opens() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested/data/app.db");
        let url = format!("sqlite://{}", file.to_str().unwrap());
        assert!(!file.exists());
        let db = connect(&ConnectOptions::new(&url)).await.unwrap();
        assert!(file.exists());
        db.ping().await.unwrap();
    }
}
