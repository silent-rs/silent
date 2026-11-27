//! 用来验证 SQLite 上各种 ALTER TABLE 能否直接完成的迁移。每条迁移都从"article 表刚建好"开始。

pub mod m20261126_000002_add_column;
pub mod m20261126_000003_rename_column;
pub mod m20261126_000004_drop_column;

use crate::common::app::m20261125_120000_create_article;
use silent_boot::db::migrate::*;

macro_rules! alter_migrator {
    ($name:ident, $migration:path) => {
        /// 先建 article 表，再执行一种 ALTER TABLE。
        #[derive(Debug, Default, Clone, Copy)]
        pub struct $name;

        impl MigratorTrait for $name {
            fn migrations() -> Vec<Box<dyn MigrationTrait>> {
                vec![
                    Box::new(m20261125_120000_create_article::Migration),
                    Box::new($migration),
                ]
            }
        }
    };
}

alter_migrator!(AddColumnMigrator, m20261126_000002_add_column::Migration);
alter_migrator!(
    RenameColumnMigrator,
    m20261126_000003_rename_column::Migration
);
alter_migrator!(DropColumnMigrator, m20261126_000004_drop_column::Migration);
