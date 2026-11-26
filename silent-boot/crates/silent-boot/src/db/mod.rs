//! 数据库接入：连接管理、具名执行锁和迁移。
//!
//! - [`connect`]：按配置建立连接池；
//! - [`lock`]：用租约行实现的具名锁，迁移和之后的定时任务共用；
//! - [`migrate`]：框架迁移与应用迁移分开跟踪，由 [`migrate::MigrationRunner`]
//!   加锁后统一执行。

pub mod connect;
pub mod lock;
pub mod migrate;

use std::collections::BTreeMap;

use sea_orm::DatabaseConnection;

pub use connect::{ConnectError, ConnectOptions};

/// 应用的数据库连接集合：一个默认连接，加若干命名连接。
///
/// handler 里通过 `State<AppState>` 取到它，再用 [`DbRegistry::db`] 或 [`DbRegistry::named`]。
#[derive(Clone, Debug)]
pub struct DbRegistry {
    default_db: DatabaseConnection,
    named: BTreeMap<String, DatabaseConnection>,
}

impl DbRegistry {
    /// 只有默认连接。
    pub fn new(default_db: DatabaseConnection) -> Self {
        Self {
            default_db,
            named: BTreeMap::new(),
        }
    }

    /// 加一个命名连接。名字 `default` 留给默认连接，重复的名字会被拒绝。
    pub fn with_named(
        mut self,
        name: impl Into<String>,
        db: DatabaseConnection,
    ) -> Result<Self, NamedConnectionError> {
        let name = name.into();
        if name.is_empty() || name == "default" || self.named.contains_key(&name) {
            return Err(NamedConnectionError(name));
        }
        self.named.insert(name, db);
        Ok(self)
    }

    /// 默认连接。
    pub fn db(&self) -> &DatabaseConnection {
        &self.default_db
    }

    /// 命名连接。
    pub fn named(&self, name: &str) -> Option<&DatabaseConnection> {
        self.named.get(name)
    }

    /// 命名连接的名字，按字典序。
    pub fn named_keys(&self) -> impl Iterator<Item = &str> {
        self.named.keys().map(String::as_str)
    }

    /// 逐个连接执行一次查询，确认数据库可用。第一个不通的连接决定错误。
    pub async fn ping_all(&self) -> Result<(), DbPingError> {
        self.default_db.ping().await.map_err(|source| DbPingError {
            name: "default".to_owned(),
            source,
        })?;
        for (name, db) in &self.named {
            db.ping().await.map_err(|source| DbPingError {
                name: name.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

/// 命名连接的名字不能用。
#[derive(Debug, thiserror::Error)]
#[error("命名连接 {0:?} 不可用：名字不能为空，不能叫 default，也不能重复")]
pub struct NamedConnectionError(pub String);

/// 某个连接 ping 不通。
#[derive(Debug, thiserror::Error)]
#[error("连接 {name} ping 失败：{source}")]
pub struct DbPingError {
    /// 连接的名字。
    pub name: String,
    /// 数据库返回的错误。
    pub source: sea_orm::DbErr,
}
