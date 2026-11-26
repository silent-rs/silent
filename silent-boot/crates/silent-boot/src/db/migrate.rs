//! 迁移：框架迁移与应用迁移分开跟踪，由 [`MigrationRunner`] 加锁后统一执行。
//!
//! 约定见 `docs/design.md` §6：
//! - 框架迁移记录在 `sb_migrations`，应用迁移记录在 SeaORM 默认的 `seaql_migrations`；
//! - 执行顺序固定为先框架、后应用，整个过程持有 `sb_locks` 表里的 `migrate` 锁（见 [`crate::db::lock`]）；
//! - 应用迁移用 [`migration!`] 声明，默认在事务里执行，SQLite 上因此可以整体回滚。
//!
//! 本模块 re-export 了 `sea-orm-migration` 的 prelude，应用迁移只需要
//! `use silent_boot::db::migrate::*;`。

pub mod framework;

use std::sync::Arc;
use std::time::Duration;

pub use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use sea_orm_migration::Migration;
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

/// [`MigratorTrait`] 的对象安全视图。
///
/// [`MigratorTrait`] 的方法都是带泛型的关联函数，没法做成 `dyn`。启动器和命令行需要在编译期
/// 不知道应用迁移器类型的地方执行迁移，所以统一走这里。任何实现了 [`MigratorTrait`] 的类型都自动实现它。
#[async_trait]
pub trait Migrator: Send + Sync {
    /// 跟踪表的名字：应用的默认是 `seaql_migrations`，框架的是 `sb_migrations`。
    fn table_name(&self) -> String;
    /// 执行待执行的迁移，`steps` 限制最多执行几条。会建跟踪表。
    async fn up(&self, db: &DatabaseConnection, steps: Option<u32>) -> Result<(), DbErr>;
    /// 建跟踪表，已经存在时什么都不做。
    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr>;
    /// 待执行的迁移名，会建跟踪表。
    async fn pending(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr>;
    /// 已执行的迁移名，不建表、不加锁。
    async fn applied(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr>;
    /// 待执行的迁移名，不建表、不加锁。跟踪表还不存在时，全部迁移都算待执行。
    async fn pending_read_only(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr>;
}

#[async_trait]
impl<T: MigratorTrait + Sync + 'static> Migrator for T {
    fn table_name(&self) -> String {
        T::migration_table_name().to_string()
    }

    async fn up(&self, db: &DatabaseConnection, steps: Option<u32>) -> Result<(), DbErr> {
        T::up(db, steps).await
    }

    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        T::install(db).await
    }

    async fn pending(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        Ok(migration_names(T::get_pending_migrations(db).await?))
    }

    async fn applied(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        Ok(migration_names(
            T::get_applied_migrations_read_only(db).await?,
        ))
    }

    async fn pending_read_only(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        Ok(migration_names(
            T::get_pending_migrations_read_only(db).await?,
        ))
    }
}

#[async_trait]
impl Migrator for AnyMigrator {
    fn table_name(&self) -> String {
        self.0.table_name()
    }

    async fn up(&self, db: &DatabaseConnection, steps: Option<u32>) -> Result<(), DbErr> {
        self.0.up(db, steps).await
    }

    async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        self.0.install(db).await
    }

    async fn pending(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.pending(db).await
    }

    async fn applied(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.applied(db).await
    }

    async fn pending_read_only(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.pending_read_only(db).await
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

    /// 见 [`Migrator::up`]。
    pub async fn up(&self, db: &DatabaseConnection, steps: Option<u32>) -> Result<(), DbErr> {
        self.0.up(db, steps).await
    }

    /// 见 [`Migrator::install`]。
    pub async fn install(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        self.0.install(db).await
    }

    /// 见 [`Migrator::pending`]。
    pub async fn pending(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.pending(db).await
    }

    /// 见 [`Migrator::applied`]。
    pub async fn applied(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.applied(db).await
    }

    /// 见 [`Migrator::pending_read_only`]。
    pub async fn pending_read_only(&self, db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
        self.0.pending_read_only(db).await
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

/// 迁移命令可能遇到的错误。
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// 没拿到执行锁。
    #[error(transparent)]
    Lock(#[from] super::lock::LockError),
    /// 数据库错误。
    #[error(transparent)]
    Db(#[from] DbErr),
    /// 结果写不出来。
    #[error("写入输出失败：{0}")]
    Io(#[from] std::io::Error),
}

/// 一组迁移（框架或应用）的执行结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GroupReport {
    /// 执行前待执行的迁移。
    pub pending_before: Vec<String>,
    /// 本次实际写入跟踪表的迁移。
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
    /// 已执行的迁移，按执行顺序。
    pub applied: Vec<String>,
    /// 待执行的迁移。
    pub pending: Vec<String>,
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
        let framework = self.run_group(db, "framework", framework, steps).await?;
        let app = self.run_group(db, "app", app, steps).await?;
        drop(guard);
        Ok(RunReport { framework, app })
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
        let pending_before = migrator.pending(db).await?;
        let result = migrator.up(db, steps).await;
        // 失败时也记录已经写入跟踪表的迁移，方便定位。
        let applied = migrator
            .applied(db)
            .await?
            .into_iter()
            .filter(|name| pending_before.contains(name))
            .collect();
        let report = GroupReport {
            pending_before,
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
        let applied = migrator.applied(db).await?;
        let pending = migrator.pending_read_only(db).await?;
        Ok(GroupStatus {
            scope,
            applied,
            pending,
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

fn migration_names(migrations: Vec<Migration>) -> Vec<String> {
    migrations
        .into_iter()
        .map(|migration| migration.name().to_owned())
        .collect()
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
