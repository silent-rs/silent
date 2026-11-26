//! M0 验证：迁移核心，对应 `docs/design.md` §16 的 V6、V13、V15。
//!
//! - V15：框架迁移与应用迁移共用一个库，各有跟踪表，执行顺序正确。
//! - V13：`migration!` 声明的迁移在 SQLite 上失败时整体回滚；不包事务的会留下半成品。
//! - V6：两个执行器并发时，执行锁保证迁移只被执行一次。
mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::app::{AppMigrator, SEED_DELAY_MS};
use common::fail::{NoTxMigrator, TxMigrator};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use silent_boot::db::migrate::{FrameworkMigrator, MigrateError, MigrationRunner, MigratorTrait};

async fn scalar(db: &DatabaseConnection, sql: &str) -> i64 {
    let stmt = Statement::from_string(DbBackend::Sqlite, sql.to_owned());
    let row = db.query_one_raw(stmt).await.expect("query");
    row.map(|row| row.try_get::<i64>("", "c").expect("column c"))
        .unwrap_or(0)
}

async fn table_exists(db: &DatabaseConnection, table: &str) -> bool {
    let sql = format!(
        "SELECT COUNT(*) AS c FROM sqlite_master WHERE type = 'table' AND name = '{table}'"
    );
    scalar(db, &sql).await == 1
}

async fn column_exists(db: &DatabaseConnection, table: &str, column: &str) -> bool {
    let sql =
        format!("SELECT COUNT(*) AS c FROM pragma_table_info('{table}') WHERE name = '{column}'");
    scalar(db, &sql).await == 1
}

async fn article_rows(db: &DatabaseConnection) -> i64 {
    scalar(db, "SELECT COUNT(*) AS c FROM \"article\"").await
}

#[tokio::test]
async fn v15_framework_and_app_migrators_share_one_database() {
    let (_dir, db) = common::temp_db().await;
    let runner = MigrationRunner::new("v15");

    // 执行前：两组都是待执行，只读查询不会建表。
    let before = MigrationRunner::status(&db, &FrameworkMigrator, &AppMigrator)
        .await
        .expect("status");
    assert_eq!(before[0].scope, "framework");
    assert_eq!(before[0].applied, Vec::<String>::new());
    assert_eq!(before[0].pending.len(), 1);
    assert_eq!(before[1].scope, "app");
    assert_eq!(before[1].pending.len(), 2);
    assert!(!table_exists(&db, "seaql_migrations").await);

    let report = runner
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("migrate up");
    assert_eq!(report.framework.applied.len(), 1);
    assert_eq!(report.app.applied.len(), 2);

    for table in [
        "sb_migrations",
        "seaql_migrations",
        "sb_migration_log",
        "sb_locks",
        "article",
    ] {
        assert!(table_exists(&db, table).await, "{table} should exist");
    }
    assert_eq!(article_rows(&db).await, 2);
    // 框架、应用各记一行成功记录。
    assert_eq!(
        scalar(
            &db,
            "SELECT COUNT(*) AS c FROM sb_migration_log WHERE status = 'success'"
        )
        .await,
        2
    );

    // 再执行一次：没有待执行的迁移，数据不会重复。
    let again = runner
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("migrate up again");
    assert!(again.is_empty());
    assert_eq!(article_rows(&db).await, 2);

    // 框架迁移先于应用迁移执行：两张跟踪表各自的记录如下。
    let after = MigrationRunner::status(&db, &FrameworkMigrator, &AppMigrator)
        .await
        .expect("status after");
    assert!(after.iter().all(|group| group.pending.is_empty()));
    assert_eq!(
        after[0].applied,
        vec!["m20261125_000001_create_sb_migration_log"]
    );
    assert_eq!(
        after[1].applied,
        vec![
            "m20261125_120000_create_article",
            "m20261125_120001_seed_article"
        ]
    );
}

#[tokio::test]
async fn v13_failed_migration_rolls_back_on_sqlite_only_inside_transaction() {
    let runner = MigrationRunner::new("v13");

    // 用 migration! 声明的迁移在事务里执行：失败后表结构不变，跟踪表里也没有这条记录。
    let (_dir, db) = common::temp_db().await;
    let err = runner
        .up(&db, &FrameworkMigrator, &TxMigrator, None)
        .await
        .expect_err("should fail");
    assert!(matches!(err, MigrateError::Db(_)), "{err:?}");
    assert!(!column_exists(&db, "article", "extra").await);
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) AS c FROM seaql_migrations").await,
        1
    );
    assert_eq!(
        scalar(
            &db,
            "SELECT COUNT(*) AS c FROM sb_migration_log WHERE status = 'failed'"
        )
        .await,
        1
    );

    // 对照：不包事务时，同一条迁移失败后列已经加上，跟踪表里却没有记录。
    let (_dir, db) = common::temp_db().await;
    let err = runner
        .up(&db, &FrameworkMigrator, &NoTxMigrator, None)
        .await
        .expect_err("should fail");
    assert!(matches!(err, MigrateError::Db(_)), "{err:?}");
    assert!(column_exists(&db, "article", "extra").await);
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) AS c FROM seaql_migrations").await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v6_lock_lets_only_one_runner_apply_migrations() {
    SEED_DELAY_MS.store(400, Ordering::Relaxed);
    let (dir, db_a) = common::temp_db().await;
    let db_b = common::connect_again(&dir).await;
    let runner_a = MigrationRunner::new("v6-a").lock_timeout(Duration::from_secs(10));
    let runner_b = MigrationRunner::new("v6-b").lock_timeout(Duration::from_secs(10));

    let (report_a, report_b) = tokio::join!(
        runner_a.up(&db_a, &FrameworkMigrator, &AppMigrator, None),
        runner_b.up(&db_b, &FrameworkMigrator, &AppMigrator, None),
    );
    SEED_DELAY_MS.store(0, Ordering::Relaxed);

    // 一个执行了全部迁移，另一个等到锁之后发现没有待执行的迁移。
    let mut applied = [
        report_a.expect("runner a").applied_total(),
        report_b.expect("runner b").applied_total(),
    ];
    applied.sort();
    assert_eq!(applied, [0, 3]);
    assert_eq!(article_rows(&db_a).await, 2);
}

/// 对照组：不走执行器、直接调用 SeaORM 的 `up` 时，两个连接会各自执行同一条待执行的迁移。
///
/// 实测结论（SQLite）：
/// - 迁移在事务里执行时，后写跟踪表的那个连接报 `UNIQUE constraint failed:
///   seaql_migrations.version`，它写入的数据随事务回滚。数据没有重复，但命令以失败告终。
/// - 迁移不包事务时，同样只有一个连接能写入跟踪表，但另一个写入的数据留了下来，
///   等于同一条迁移被执行了两次，而且没有任何提示。
///
/// 只用来复现风险，不进 CI。手动运行：
/// `cargo test -p silent-boot --test migration v6_repro -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "对照组，按需手动运行"]
async fn v6_repro_two_runners_without_lock() {
    use common::app::NoTxSeedMigrator;

    // 场景一：迁移在事务里执行（`migration!` 的默认值）。
    SEED_DELAY_MS.store(400, Ordering::Relaxed);
    let (dir, db_a) = common::temp_db().await;
    let db_b = common::connect_again(&dir).await;
    AppMigrator::up(&db_a, Some(1)).await.expect("create table");
    let (result_a, result_b) =
        tokio::join!(AppMigrator::up(&db_a, None), AppMigrator::up(&db_b, None),);
    SEED_DELAY_MS.store(0, Ordering::Relaxed);
    let rows = article_rows(&db_a).await;
    println!("in transaction: a={result_a:?} b={result_b:?} article rows = {rows}");
    assert_ne!(
        result_a.is_err(),
        result_b.is_err(),
        "exactly one runner should fail"
    );
    assert_eq!(rows, 2);

    // 场景二：迁移不包事务（SeaORM 在 SQLite 上的默认值）。
    SEED_DELAY_MS.store(400, Ordering::Relaxed);
    let (dir, db_a) = common::temp_db().await;
    let db_b = common::connect_again(&dir).await;
    NoTxSeedMigrator::up(&db_a, Some(1))
        .await
        .expect("create table");
    let (result_a, result_b) = tokio::join!(
        NoTxSeedMigrator::up(&db_a, None),
        NoTxSeedMigrator::up(&db_b, None),
    );
    SEED_DELAY_MS.store(0, Ordering::Relaxed);
    let rows = article_rows(&db_a).await;
    println!("without transaction: a={result_a:?} b={result_b:?} article rows = {rows}");
    assert_ne!(
        result_a.is_err(),
        result_b.is_err(),
        "exactly one runner should fail"
    );
    assert_eq!(rows, 4, "the seed migration should have been applied twice");
}
