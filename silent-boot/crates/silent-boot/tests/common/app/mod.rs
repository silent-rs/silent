//! 样例应用的迁移：建 article 表，再写入样例数据。

pub mod m20261125_120000_create_article;
pub mod m20261125_120001_seed_article;
pub mod m20261125_120003_seed_article_no_txn;

use std::sync::atomic::{AtomicU64, Ordering};

use silent_boot::db::migrate::*;

/// 写样例数据前的等待毫秒数。默认 0；并发测试用它放大"读到待执行到真正写入"之间的窗口。
pub static SEED_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// 样例应用的迁移器：跟踪表用 SeaORM 默认的 `seaql_migrations`。
#[derive(Debug, Default, Clone, Copy)]
pub struct AppMigrator;

impl MigratorTrait for AppMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261125_120000_create_article::Migration),
            Box::new(m20261125_120001_seed_article::Migration),
        ]
    }
}

/// 和 `AppMigrator` 相同，但写数据的那条迁移不包事务（SeaORM 在 SQLite 上的默认值），只用于对照。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoTxSeedMigrator;

impl MigratorTrait for NoTxSeedMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261125_120000_create_article::Migration),
            Box::new(m20261125_120003_seed_article_no_txn::Migration),
        ]
    }
}

pub async fn seed_delay() {
    let delay = SEED_DELAY_MS.load(Ordering::Relaxed);
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }
}
