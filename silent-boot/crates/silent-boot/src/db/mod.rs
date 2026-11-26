//! 数据库接入：连接管理与迁移。
//!
//! 迁移部分见 [`migrate`]：框架迁移与应用迁移分开跟踪，由 [`migrate::MigrationRunner`]
//! 加锁后统一执行。

pub mod migrate;

use sea_orm::DatabaseConnection;

/// 应用的数据库连接集合。
///
/// M0 阶段只有默认连接；命名连接随 M1 补齐。
#[derive(Clone, Debug)]
pub struct DbRegistry {
    default_db: DatabaseConnection,
}

impl DbRegistry {
    /// 用默认连接创建。
    pub fn new(default_db: DatabaseConnection) -> Self {
        Self { default_db }
    }

    /// 默认连接。
    pub fn db(&self) -> &DatabaseConnection {
        &self.default_db
    }
}
