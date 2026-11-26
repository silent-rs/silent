//! 迁移核心，对应 `docs/design.md` §16 的 V6、V13、V15，以及 M1 的"旧版本建库后升级"。
//!
//! - 升级：旧版本的迁移列表建的库，写入数据之后由新版本接着升级，数据保留，旧版本照样能跑。
//! - V15：框架迁移与应用迁移共用一个库，各有跟踪表，执行顺序正确。
//! - V13：`migration!` 声明的迁移在 SQLite 上失败时整体回滚；不包事务的会留下半成品。
//! - V6：两个执行器并发时，执行锁保证迁移只被执行一次。
mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::app::{AppMigrator, SEED_DELAY_MS};
use common::fail::{NoTxMigrator, TxMigrator};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use silent_boot::db::DbRegistry;
use silent_boot::db::migrate::{
    FrameworkMigrator, MigrateError, MigrationRunner, MigrationTrait, MigratorTrait,
};
use silent_boot::web::Readiness;

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

/// 旧版本：只认建表那一条迁移。
#[derive(Debug, Default, Clone, Copy)]
struct ReleaseV1;

impl MigratorTrait for ReleaseV1 {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(
            common::app::m20261125_120000_create_article::Migration,
        )]
    }
}

/// 新版本：多了一条加列的迁移。
#[derive(Debug, Default, Clone, Copy)]
struct ReleaseV2;

impl MigratorTrait for ReleaseV2 {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(common::app::m20261125_120000_create_article::Migration),
            Box::new(common::alter::m20261126_000002_add_column::Migration),
        ]
    }
}

const ADD_COLUMN: &str = "m20261126_000002_add_column";

#[tokio::test]
async fn upgrading_a_database_created_by_an_old_release_keeps_the_data() {
    let (_dir, db) = common::temp_db().await;

    // 旧版本建库，跑了一段时间，写下了数据。
    MigrationRunner::new("v1")
        .up(&db, &FrameworkMigrator, &ReleaseV1, None)
        .await
        .expect("v1 builds the database");
    db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO article (id, title, status, created_at, updated_at) \
         VALUES ('a1', '旧数据', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00')"
            .to_owned(),
    ))
    .await
    .expect("insert");
    assert!(!column_exists(&db, "article", "extra").await);

    // 新版本的二进制接手：先看到一条待执行的迁移；执行之后列有了，数据还在。
    let status = MigrationRunner::status(&db, &FrameworkMigrator, &ReleaseV2)
        .await
        .expect("status");
    assert_eq!(status[1].pending, vec![ADD_COLUMN.to_owned()]);
    let report = MigrationRunner::new("v2")
        .up(&db, &FrameworkMigrator, &ReleaseV2, None)
        .await
        .expect("v2 upgrades the database");
    assert_eq!(report.app.applied, vec![ADD_COLUMN.to_owned()]);
    assert!(report.framework.applied.is_empty());
    assert!(column_exists(&db, "article", "extra").await);
    assert_eq!(article_rows(&db).await, 1);
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) AS c FROM seaql_migrations").await,
        2
    );

    // 滚动发布期间还在跑的旧副本：不认识新迁移，不报错，也不会重复执行任何东西。
    let old = MigrationRunner::new("v1-again")
        .up(&db, &FrameworkMigrator, &ReleaseV1, None)
        .await
        .expect("the old release still runs");
    assert!(old.is_empty());
    assert_eq!(article_rows(&db).await, 1);

    // 旧副本的状态里把不认识的迁移列成"数据库比这个版本新"，不算待执行；就绪检查照样通过。
    let status = MigrationRunner::status(&db, &FrameworkMigrator, &ReleaseV1)
        .await
        .expect("status of the old release");
    assert!(status[1].pending.is_empty());
    assert_eq!(status[1].unknown, vec![ADD_COLUMN.to_owned()]);
    let readiness = Readiness::new(DbRegistry::new(db.clone()), FrameworkMigrator, ReleaseV1);
    assert!(
        readiness.check().await.ready,
        "an old replica stays ready after the database was upgraded"
    );
}

/// 一个和线上分叉的版本：既缺线上已经执行的迁移，又有自己没执行过的迁移。
#[derive(Debug, Default, Clone, Copy)]
struct SkewedRelease;

impl MigratorTrait for SkewedRelease {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(common::app::m20261125_120000_create_article::Migration),
            Box::new(common::alter::m20261126_000003_rename_column::Migration),
        ]
    }
}

#[tokio::test]
async fn a_release_that_diverged_from_the_database_refuses_to_migrate() {
    let (_dir, db) = common::temp_db().await;
    MigrationRunner::new("v2")
        .up(&db, &FrameworkMigrator, &ReleaseV2, None)
        .await
        .expect("v2 builds the database");

    // 分叉的版本：不认识 add_column，又想执行 rename_column。
    let err = MigrationRunner::new("skewed")
        .up(&db, &FrameworkMigrator, &SkewedRelease, None)
        .await
        .expect_err("a diverged release must not migrate");
    assert!(matches!(err, MigrateError::DatabaseIsNewer(_)), "{err:?}");
    let text = err.to_string();
    assert!(text.contains(ADD_COLUMN), "{text}");
    assert!(text.contains("m20261126_000003_rename_column"), "{text}");
    // 它自己的迁移没有被执行。
    assert!(!column_exists(&db, "article", "extra_renamed").await);
    assert!(column_exists(&db, "article", "extra").await);
}
