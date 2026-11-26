//! 迁移：框架迁移与应用迁移分开跟踪，由 [`MigrationRunner`] 加锁后统一执行。
//!
//! 约定见 `docs/design.md` §6：
//! - 框架迁移记录在 `sb_migrations`，应用迁移记录在 SeaORM 默认的 `seaql_migrations`；
//! - 执行顺序固定为先框架、后应用，整个过程持有 `sb_locks` 表里的 `migrate` 锁（见 [`crate::db::lock`]）；
//! - 应用迁移用 [`migration!`] 声明，默认在事务里执行，SQLite 上因此可以整体回滚。
//!
//! 对账的方式让滚动发布可行：待执行的迁移是"这个版本里有、跟踪表里还没有"的那些。跟踪表里有、
//! 这个版本却不认识的迁移（数据库被更新的版本升级过）不算问题，旧副本照常就绪、照常启动；
//! SeaORM 自己的 `up` 遇到这种情况会直接报错，所以执行器不调用它，而是算好待执行的清单再交给它执行。
//!
//! 本模块 re-export 了 `sea-orm-migration` 的 prelude，应用迁移只需要
//! `use silent_boot::db::migrate::*;`。

pub mod framework;

use std::sync::Arc;
use std::time::Duration;

pub use async_trait::async_trait;
use sea_orm::{ConnectionTrait as _, DatabaseConnection, TransactionTrait as _};
pub use sea_orm_migration::prelude::*;

use super::lock::{self, LockOptions};

/// 迁移执行器用的锁名，见 [`crate::db::lock`]。
const LOCK_NAME: &str = "migrate";

/// 框架迁移：系统表和框架自身需要的结构。
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameworkMigrator;

impl MigratorTrait for FrameworkMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(
            framework::m20261125_000001_create_sb_migration_log::Migration,
        )]
    }

    fn migration_table_name() -> DynIden {
        Alias::new("sb_migrations").into_iden()
    }
}

/// 没有任何迁移的迁移器：应用自己没有迁移，或者只需要框架迁移时用它。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMigrations;

impl MigratorTrait for NoMigrations {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Vec::new()
    }
}

/// 一组迁移和它的跟踪表对账的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct MigrationDiff {
    /// 这个版本里的迁移，已经记在跟踪表里的，按执行顺序。
    pub applied: Vec<String>,
    /// 这个版本里的迁移，跟踪表里还没有的，按执行顺序。
    pub pending: Vec<String>,
    /// 跟踪表里有、这个版本却不认识的：数据库已经被更新的版本升级过。
    pub unknown: Vec<String>,
}

impl MigrationDiff {
    /// `migrations` 是这个版本里的全部迁移（按执行顺序），`recorded` 是跟踪表里的版本。
    pub fn between(migrations: &[String], recorded: &[String]) -> Self {
        Self {
            applied: migrations
                .iter()
                .filter(|name| recorded.contains(name))
                .cloned()
                .collect(),
            pending: migrations
                .iter()
                .filter(|name| !recorded.contains(name))
                .cloned()
                .collect(),
            unknown: recorded
                .iter()
                .filter(|name| !migrations.contains(name))
                .cloned()
                .collect(),
        }
    }
}

/// [`MigratorTrait`] 的对象安全视图。
///
/// [`MigratorTrait`] 的方法都是带泛型的关联函数，没法做成 `dyn`。启动器和命令行需要在编译期
/// 不知道应用迁移器类型的地方执行迁移，所以统一走这里。任何实现了 [`MigratorTrait`] 的类型都自动实现它。
#[async_trait]
pub trait Migrator: Send + Sync {
    /// 跟踪表的名字：应用的默认是 `seaql_migrations`，框架的是 `sb_migrations`。
    fn table_name(&self) -> String;
    /// 这个版本里的全部迁移，按执行顺序。
    fn migration_names(&self) -> Vec<String>;
    /// 跟踪表里记下的版本，按执行顺序。不建表：表还不存在时是空的。
    async fn recorded(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr>;
    /// 执行给定的迁移并写跟踪表，`steps` 限制最多执行几条。
    ///
    /// `migrations` 必须是 [`Migrator::migration_names`] 的子集，按那个顺序执行。
    async fn apply(
        &self,
        db: &DatabaseConnection,
        migrations: &[String],
        steps: Option<u32>,
    ) -> Result<(), DbErr>;
    /// 建跟踪表，已经存在时什么都不做。
    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr>;

    /// 和跟踪表对账。不建表、不加锁。
    async fn diff(&self, db: &DatabaseConnection) -> Result<MigrationDiff, DbErr> {
        let recorded = self.recorded(db).await?;
        Ok(MigrationDiff::between(&self.migration_names(), &recorded))
    }
}

#[async_trait]
impl<T: MigratorTrait + Sync + 'static> Migrator for T {
    fn table_name(&self) -> String {
        T::migration_table_name().to_string()
    }

    fn migration_names(&self) -> Vec<String> {
        T::migrations()
            .iter()
            .map(|migration| migration.name().to_owned())
            .collect()
    }

    async fn recorded(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        recorded(db, &T::migration_table_name()).await
    }

    async fn apply(
        &self,
        db: &DatabaseConnection,
        migrations: &[String],
        steps: Option<u32>,
    ) -> Result<(), DbErr> {
        let table = T::migration_table_name();
        T::install(db).await?;
        let backend = db.get_database_backend();
        let mut left = steps;
        for migration in T::migrations() {
            let name = migration.name().to_owned();
            if !migrations.contains(&name) {
                continue;
            }
            if let Some(left) = left.as_mut() {
                if *left == 0 {
                    break;
                }
                *left -= 1;
            }
            // 和 SeaORM 的约定一致：迁移自己没说、又只有 PostgreSQL 的 DDL 能回滚时，才放进事务。
            let use_txn = migration
                .use_transaction()
                .unwrap_or(backend == sea_orm::DatabaseBackend::Postgres);
            if use_txn {
                let txn = db.begin().await?;
                migration.up(&SchemaManager::new(&txn)).await?;
                insert_record(&txn, &table, &name).await?;
                txn.commit().await?;
            } else {
                migration.up(&SchemaManager::new(db)).await?;
                insert_record(db, &table, &name).await?;
            }
        }
        Ok(())
    }

    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        T::install(db).await
    }
}

#[async_trait]
impl Migrator for AnyMigrator {
    fn table_name(&self) -> String {
        self.0.table_name()
    }

    fn migration_names(&self) -> Vec<String> {
        self.0.migration_names()
    }

    async fn recorded(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.recorded(db).await
    }

    async fn apply(
        &self,
        db: &DatabaseConnection,
        migrations: &[String],
        steps: Option<u32>,
    ) -> Result<(), DbErr> {
        self.0.apply(db, migrations, steps).await
    }

    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        self.0.install(db).await
    }
}

/// 持有任意一个迁移器：启动器和命令行用它保存框架迁移器与应用迁移器。
#[derive(Clone)]
pub struct AnyMigrator(Arc<dyn Migrator>);

impl std::fmt::Debug for AnyMigrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnyMigrator")
    }
}

impl AnyMigrator {
    /// 包一个迁移器。实现了 [`MigratorTrait`] 的类型都可以直接传进来。
    pub fn new<M: Migrator + 'static>(migrator: M) -> Self {
        Self(Arc::new(migrator))
    }

    /// 框架自己的迁移器。
    pub fn framework() -> Self {
        Self::new(FrameworkMigrator)
    }

    /// 见 [`Migrator::table_name`]。
    pub fn table_name(&self) -> String {
        self.0.table_name()
    }

    /// 见 [`Migrator::migration_names`]。
    pub fn migration_names(&self) -> Vec<String> {
        self.0.migration_names()
    }

    /// 见 [`Migrator::recorded`]。
    pub async fn recorded(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.recorded(db).await
    }

    /// 见 [`Migrator::diff`]。
    pub async fn diff(&self, db: &DatabaseConnection) -> Result<MigrationDiff, DbErr> {
        self.0.diff(db).await
    }

    /// 见 [`Migrator::install`]。
    pub async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        self.0.install(db).await
    }

    /// 当成 trait 对象用。
    pub fn as_dyn(&self) -> &dyn Migrator {
        &*self.0
    }
}

impl<T: MigratorTrait + Sync + 'static> From<T> for AnyMigrator {
    fn from(migrator: T) -> Self {
        Self::new(migrator)
    }
}

/// 数据库比这个版本新，这个版本又有待执行的迁移：不能用旧版本接着升级。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseIsNewer {
    /// 跟踪表里有、这个版本不认识的迁移。
    pub unknown: Vec<String>,
    /// 这个版本里待执行的迁移。
    pub pending: Vec<String>,
}

impl std::error::Error for DatabaseIsNewer {}

impl std::fmt::Display for DatabaseIsNewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "数据库里有 {} 条迁移这个版本不认识（例如 {}），这个版本又有 {} 条待执行的迁移（例如 {}）。\
             不要用旧版本执行迁移：换最新的版本执行",
            self.unknown.len(),
            self.unknown.first().map(String::as_str).unwrap_or_default(),
            self.pending.len(),
            self.pending.first().map(String::as_str).unwrap_or_default()
        )
    }
}

/// 迁移命令可能遇到的错误。
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// 没拿到执行锁。
    #[error(transparent)]
    Lock(#[from] super::lock::LockError),
    /// 数据库错误。
    #[error(transparent)]
    Db(#[from] DbErr),
    /// 数据库比这个版本新，这个版本又有待执行的迁移。
    #[error(transparent)]
    DatabaseIsNewer(#[from] DatabaseIsNewer),
    /// 结果写不出来。
    #[error("写入输出失败：{0}")]
    Io(#[from] std::io::Error),
}

/// 一次 `migrate up` 里一组的执行结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GroupReport {
    /// 执行前待执行的迁移。
    pub pending_before: Vec<String>,
    /// 本次实际执行并记进跟踪表的迁移。
    pub applied: Vec<String>,
}

impl GroupReport {
    fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }
}

/// 一次 `migrate up` 的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct RunReport {
    /// 框架迁移这一组。
    pub framework: GroupReport,
    /// 应用迁移这一组。
    pub app: GroupReport,
}

impl RunReport {
    /// 两组本次实际执行的迁移总数。
    pub fn applied_total(&self) -> usize {
        self.framework.applied.len() + self.app.applied.len()
    }

    /// 两组是否都没有执行任何迁移。
    pub fn is_empty(&self) -> bool {
        self.framework.is_empty() && self.app.is_empty()
    }
}

/// 一组迁移的状态，供 `migrate status` 使用。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GroupStatus {
    /// `framework` 或 `app`。
    pub scope: &'static str,
    /// 这个版本里的迁移里，已经执行的，按执行顺序。
    pub applied: Vec<String>,
    /// 待执行的迁移，按执行顺序。
    pub pending: Vec<String>,
    /// 跟踪表里有、这个版本不认识的迁移：数据库被更新的版本升级过。
    pub unknown: Vec<String>,
}

/// 迁移执行器。
///
/// 同一个实例名可以重入；不同实例名之间通过执行锁串行。
#[derive(Debug, Clone)]
pub struct MigrationRunner {
    instance: String,
    lock_timeout: Duration,
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self::new(default_instance())
    }
}

impl MigrationRunner {
    /// `instance` 用来标识执行者，写进锁和执行记录。
    pub fn new(instance: impl Into<String>) -> Self {
        Self {
            instance: instance.into(),
            lock_timeout: Duration::from_secs(30),
        }
    }

    /// 设置等待执行锁的最长时间，默认 30 秒。
    pub fn lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// 先执行框架迁移，再执行应用迁移，全程持有执行锁。
    ///
    /// `steps` 对两组分别生效。任何一组失败都会立刻返回，已经执行的迁移不会回滚。
    /// 数据库比这个版本新、这个版本又有待执行的迁移时拒绝执行，见 [`MigrateError::DatabaseIsNewer`]。
    pub async fn up(
        &self,
        db: &DatabaseConnection,
        framework: &dyn Migrator,
        app: &dyn Migrator,
        steps: Option<u32>,
    ) -> Result<RunReport, MigrateError> {
        let options = LockOptions {
            timeout: self.lock_timeout,
            ..LockOptions::default()
        };
        let guard = lock::acquire(db, LOCK_NAME, &self.instance, &options).await?;
        let result = async {
            let framework = self.run_group(db, "framework", framework, steps).await?;
            let app = self.run_group(db, "app", app, steps).await?;
            Ok(RunReport { framework, app })
        }
        .await;
        // 显式释放：命令执行完进程可能马上退出，靠 Drop 里的后台任务不可靠。
        guard.release().await;
        result
    }

    /// 只读查询两组迁移的状态：不建表、不加锁。
    pub async fn status(
        db: &DatabaseConnection,
        framework: &dyn Migrator,
        app: &dyn Migrator,
    ) -> Result<Vec<GroupStatus>, MigrateError> {
        Ok(vec![
            Self::group_status(db, "framework", framework).await?,
            Self::group_status(db, "app", app).await?,
        ])
    }

    async fn run_group(
        &self,
        db: &DatabaseConnection,
        scope: &str,
        migrator: &dyn Migrator,
        steps: Option<u32>,
    ) -> Result<GroupReport, MigrateError> {
        let before = migrator.diff(db).await?;
        if !before.unknown.is_empty() && !before.pending.is_empty() {
            return Err(DatabaseIsNewer {
                unknown: before.unknown,
                pending: before.pending,
            }
            .into());
        }
        let result = if before.pending.is_empty() {
            Ok(())
        } else {
            migrator.apply(db, &before.pending, steps).await
        };
        // 失败时也记录已经写进跟踪表的迁移，方便定位。
        let after = migrator.diff(db).await?;
        let applied = before
            .pending
            .iter()
            .filter(|name| after.applied.contains(name))
            .cloned()
            .collect();
        let report = GroupReport {
            pending_before: before.pending,
            applied,
        };
        self.log(db, scope, &report, result.as_ref().err()).await;
        result?;
        Ok(report)
    }

    async fn group_status(
        db: &DatabaseConnection,
        scope: &'static str,
        migrator: &dyn Migrator,
    ) -> Result<GroupStatus, MigrateError> {
        let diff = migrator.diff(db).await?;
        Ok(GroupStatus {
            scope,
            applied: diff.applied,
            pending: diff.pending,
            unknown: diff.unknown,
        })
    }

    /// 写一行执行记录。框架迁移自己失败时这张表可能还不存在，写不进去也不影响结果。
    async fn log(
        &self,
        db: &DatabaseConnection,
        scope: &str,
        report: &GroupReport,
        error: Option<&DbErr>,
    ) {
        let status = if error.is_some() { "failed" } else { "success" };
        let stmt = Query::insert()
            .into_table(Alias::new("sb_migration_log"))
            .columns([
                Alias::new("id"),
                Alias::new("scope"),
                Alias::new("instance"),
                Alias::new("pending_before"),
                Alias::new("applied_after"),
                Alias::new("status"),
                Alias::new("started_at"),
                Alias::new("finished_at"),
                Alias::new("error"),
            ])
            .values_panic([
                scru128::new_string().into(),
                scope.to_owned().into(),
                self.instance.clone().into(),
                to_json(&report.pending_before).into(),
                to_json(&report.applied).into(),
                status.to_owned().into(),
                now_local().into(),
                Some(now_local()).into(),
                error.map(|e| e.to_string()).into(),
            ])
            .to_owned();
        let _ = db.execute(&stmt).await;
    }
}

fn default_instance() -> String {
    std::env::var("INSTANCE_ID").unwrap_or_else(|_| format!("pid-{}", std::process::id()))
}

/// 跟踪表里记下的版本，按执行顺序。表还不存在时是空的，不建表。
async fn recorded(db: &DatabaseConnection, table: &DynIden) -> Result<Vec<String>, DbErr> {
    // 先用只读的探测确认表在不在：直接查的话，三种数据库报"表不存在"的原文不一样。
    if !SchemaManager::new(db).has_table(table.to_string()).await? {
        return Ok(Vec::new());
    }
    let stmt = Query::select()
        .column(Alias::new("version"))
        .from(table.clone())
        .order_by(Alias::new("version"), Order::Asc)
        .to_owned();
    let rows = db.query_all(&stmt).await?;
    rows.iter()
        .map(|row| row.try_get::<String>("", "version"))
        .collect()
}

/// 写一行跟踪记录。`applied_at` 和 SeaORM 自己写的一样，是 Unix 秒。
///
/// SeaORM 的 `exec_up_with` 没有公开，而且它的 `up` 见到跟踪表里有这个版本不认识的迁移就报错，
/// 所以执行循环由框架自己写，只借用 SeaORM 的迁移 trait 和建表逻辑。
async fn insert_record<C: ConnectionTrait>(
    db: &C,
    table: &DynIden,
    version: &str,
) -> Result<(), DbErr> {
    let stmt = Query::insert()
        .into_table(table.clone())
        .columns([Alias::new("version"), Alias::new("applied_at")])
        .values_panic([
            version.to_owned().into(),
            chrono::Utc::now().timestamp().into(),
        ])
        .to_owned();
    db.execute(&stmt).await?;
    Ok(())
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn now_local() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// 声明一条迁移：生成带文件名版本号的结构体，并默认在事务里执行。
///
/// 每条迁移放在单独的文件里，文件名形如 `m20261125_120000_create_article.rs`，
/// 版本号取自文件名。`down` 可以省略，省略后回滚这条迁移会返回错误。
///
/// ```ignore
/// use silent_boot::db::migrate::*;
///
/// silent_boot::migration! {
///     /// 创建 article 表
///     pub struct Migration;
///
///     async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
///         manager.get_connection().execute_unprepared("...").await
///     }
/// }
/// ```
#[macro_export]
macro_rules! migration {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;

        async fn up($manager:ident: $manager_ty:ty) -> Result<(), DbErr> {
            $($up:tt)*
        }
    ) => {
        $crate::migration!(
            @impl $(#[$meta])* $vis $name
            up $manager: $manager_ty { $($up)* }
            down _manager: $manager_ty {
                Err($crate::db::migrate::sea_orm::DbErr::Migration(
                    "down is not implemented for this migration".to_owned(),
                ))
            }
        );
    };
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;

        async fn up($up_manager:ident: $up_ty:ty) -> Result<(), DbErr> {
            $($up:tt)*
        }

        async fn down($down_manager:ident: $down_ty:ty) -> Result<(), DbErr> {
            $($down:tt)*
        }
    ) => {
        $crate::migration!(
            @impl $(#[$meta])* $vis $name
            up $up_manager: $up_ty { $($up)* }
            down $down_manager: $down_ty { $($down)* }
        );
    };
    (
        @impl
        $(#[$meta:meta])*
        $vis:vis $name:ident
        up $up_manager:ident: $up_ty:ty { $($up:tt)* }
        down $down_manager:ident: $down_ty:ty { $($down:tt)* }
    ) => {
        $(#[$meta])*
        #[derive($crate::db::migrate::DeriveMigrationName)]
        $vis struct $name;

        #[$crate::db::migrate::async_trait]
        impl $crate::db::migrate::MigrationTrait for $name {
            async fn up(
                &self,
                $up_manager: $up_ty,
            ) -> Result<(), $crate::db::migrate::sea_orm::DbErr> {
                $($up)*
            }

            async fn down(
                &self,
                $down_manager: $down_ty,
            ) -> Result<(), $crate::db::migrate::sea_orm::DbErr> {
                $($down)*
            }

            fn use_transaction(&self) -> Option<bool> {
                Some(true)
            }
        }
    };
}
