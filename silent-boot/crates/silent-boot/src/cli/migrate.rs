//! `migrate` 子命令：执行迁移并把结果打印出来。
//!
//! 数据库连接由应用提供（来自应用配置），所以这里不读环境变量。
//! `F` 是框架迁移器，`A` 是应用自己的迁移器。

use crate::db::migrate::{GroupStatus, MigrateError, MigrationRunner, MigratorTrait};
use sea_orm::DatabaseConnection;

/// `migrate up`：加锁后先执行框架迁移，再执行应用迁移。
pub async fn up<F, A>(
    db: &DatabaseConnection,
    steps: Option<u32>,
    dry_run: bool,
) -> Result<(), MigrateError>
where
    F: MigratorTrait,
    A: MigratorTrait,
{
    if dry_run {
        let groups = MigrationRunner::status::<F, A>(db).await?;
        print_groups(&groups, false);
        return Ok(());
    }
    let report = MigrationRunner::default().up::<F, A>(db, steps).await?;
    for (scope, applied) in [
        ("framework", &report.framework.applied),
        ("app", &report.app.applied),
    ] {
        println!("{scope}: {} applied", applied.len());
        for name in applied {
            println!("  {name}");
        }
    }
    Ok(())
}

/// `migrate status`：只读查询，不建表、不加锁。
pub async fn status<F, A>(db: &DatabaseConnection, json: bool) -> Result<(), MigrateError>
where
    F: MigratorTrait,
    A: MigratorTrait,
{
    let groups = MigrationRunner::status::<F, A>(db).await?;
    if json {
        // `GroupStatus` 只有字符串和字符串数组，序列化不会失败。
        println!(
            "{}",
            serde_json::to_string_pretty(&groups).expect("status is serializable")
        );
        return Ok(());
    }
    print_groups(&groups, true);
    Ok(())
}

fn print_groups(groups: &[GroupStatus], show_applied: bool) {
    for group in groups {
        println!("{}", group.scope);
        if show_applied {
            for name in &group.applied {
                println!("  applied  {name}");
            }
        }
        for name in &group.pending {
            println!("  pending  {name}");
        }
        if group.pending.is_empty() && (!show_applied || group.applied.is_empty()) {
            println!("  up to date");
        }
    }
}
