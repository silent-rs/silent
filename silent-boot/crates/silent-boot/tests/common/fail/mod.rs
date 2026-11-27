//! 用来验证失败行为的迁移：先改表，再故意失败。

pub mod m20261125_120001_alter_then_fail;
pub mod m20261125_120002_alter_then_fail_no_txn;

use crate::common::app::m20261125_120000_create_article;
use silent_boot::db::migrate::*;

/// 先建表，再执行一条"改了表之后失败"的迁移。失败的那条用 `migration!` 声明，在事务里执行。
#[derive(Debug, Default, Clone, Copy)]
pub struct TxMigrator;

impl MigratorTrait for TxMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261125_120000_create_article::Migration),
            Box::new(m20261125_120001_alter_then_fail::Migration),
        ]
    }
}

/// 同样的两条迁移，但失败的那条没有覆盖 `use_transaction`，沿用 SeaORM 的默认约定。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoTxMigrator;

impl MigratorTrait for NoTxMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261125_120000_create_article::Migration),
            Box::new(m20261125_120002_alter_then_fail_no_txn::Migration),
        ]
    }
}
