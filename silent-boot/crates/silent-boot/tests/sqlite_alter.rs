//! M0 验证：V13 的 ALTER 边界，SQLite 上哪些表结构变更能直接用迁移完成。
//!
//! 结论（实测）：
//! - 加列、改列名、删列都能直接完成。
//! - 改列的类型或约束做不到：sea-query 在为 SQLite 生成这类语句时直接 panic，而不是返回错误。
//!   要改类型只能重建表。
mod common;

use common::alter::{AddColumnMigrator, DropColumnMigrator, RenameColumnMigrator};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use silent_boot::db::migrate::{Alias, ColumnDef, FrameworkMigrator, MigrationRunner, Table};

async fn column_names(db: &DatabaseConnection, table: &str) -> Vec<String> {
    let stmt = Statement::from_string(
        DbBackend::Sqlite,
        format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"),
    );
    db.query_all_raw(stmt)
        .await
        .expect("query")
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").expect("name"))
        .collect()
}

async fn sqlite_version(db: &DatabaseConnection) -> String {
    let stmt = Statement::from_string(DbBackend::Sqlite, "SELECT sqlite_version() AS v");
    let row = db.query_one_raw(stmt).await.expect("query").expect("row");
    row.try_get::<String>("", "v").expect("v")
}

#[tokio::test]
async fn v13_sqlite_can_add_rename_and_drop_columns() {
    let runner = MigrationRunner::new("v13-alter");

    let (_dir, db) = common::temp_db().await;
    runner
        .up(&db, &FrameworkMigrator, &AddColumnMigrator, None)
        .await
        .expect("add column");
    assert!(
        column_names(&db, "article")
            .await
            .contains(&"extra".to_owned())
    );

    let (_dir, db) = common::temp_db().await;
    runner
        .up(&db, &FrameworkMigrator, &RenameColumnMigrator, None)
        .await
        .expect("rename column");
    let columns = column_names(&db, "article").await;
    assert!(columns.contains(&"extra_renamed".to_owned()));
    assert!(!columns.contains(&"extra".to_owned()));

    let (_dir, db) = common::temp_db().await;
    runner
        .up(&db, &FrameworkMigrator, &DropColumnMigrator, None)
        .await
        .expect("drop column");
    assert!(
        !column_names(&db, "article")
            .await
            .contains(&"extra".to_owned())
    );
    // 删列要 SQLite 3.35.0 起才支持，记下实际使用的版本。
    println!("sqlite version: {}", sqlite_version(&db).await);
}

#[test]
fn v13_sqlite_panics_instead_of_erroring_when_modifying_a_column() {
    // 静音 panic 输出，只断言它确实发生了。
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        let stmt = Table::alter()
            .table(Alias::new("article"))
            .modify_column(ColumnDef::new(Alias::new("title")).text())
            .to_owned();
        DbBackend::Sqlite.build(&stmt).to_string()
    });
    std::panic::set_hook(previous);
    assert!(
        result.is_err(),
        "expected a panic while rendering the statement"
    );
}
