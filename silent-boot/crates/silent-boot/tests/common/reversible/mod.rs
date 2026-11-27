//! 两条都能回退的迁移，给 `migrate down` 的测试用：建 `tag` 表、建 `note` 表。
pub mod m20261127_000001_create_tag;
pub mod m20261127_000002_create_note;

use silent_boot::db::migrate::*;

/// 两条迁移都有 `down`。
#[derive(Debug, Default, Clone, Copy)]
pub struct ReversibleMigrator;

impl MigratorTrait for ReversibleMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261127_000001_create_tag::Migration),
            Box::new(m20261127_000002_create_note::Migration),
        ]
    }
}

/// 只认第一条：用来模拟"数据库被更新的版本升级过"的旧版本。
#[derive(Debug, Default, Clone, Copy)]
pub struct OnlyTagMigrator;

impl MigratorTrait for OnlyTagMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261127_000001_create_tag::Migration)]
    }
}
