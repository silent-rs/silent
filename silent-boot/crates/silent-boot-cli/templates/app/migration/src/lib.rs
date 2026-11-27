//! 应用的迁移器。
//!
//! - 新增迁移：`cargo run -- migrate generate "create order"`，会生成文件并登记到下面两个受管区块；
//! - 执行迁移：`cargo run -- migrate up`；查看状态：`cargo run -- migrate status`；
//! - 框架自己的迁移由 silent-boot 负责，记录在 `sb_migrations`，和应用迁移互不影响。

use silent_boot::db::migrate::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            // <silent-boot:gen:list>
            Box::new(m20261126_000001_create_article::Migration),
            // </silent-boot:gen:list>
        ]
    }
}

// <silent-boot:gen:mods>
pub mod m20261126_000001_create_article;
// </silent-boot:gen:mods>
