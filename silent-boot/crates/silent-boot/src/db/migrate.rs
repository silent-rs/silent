//! 迁移：框架迁移与应用迁移分开跟踪，由 [`MigrationRunner`] 加锁后统一执行。
//!
//! 约定见 `docs/design.md` §6：
//! - 框架迁移记录在 `sb_migrations`，应用迁移记录在 SeaORM 默认的 `seaql_migrations`；
//! - 执行顺序固定为先框架、后应用，整个过程持有 `sb_migration_lock` 里的执行锁；
//! - 应用迁移用 [`migration!`] 声明，默认在事务里执行，SQLite 上因此可以整体回滚。
//!
//! 本模块 re-export 了 `sea-orm-migration` 的 prelude，应用迁移只需要
//! `use silent_boot::db::migrate::*;`。

pub mod framework;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use sea_orm_migration::Migration;
pub use sea_orm_migration::prelude::*;

/// 执行锁所在的表。
const LOCK_TABLE: &str = "sb_migration_lock";
/// 执行锁所在表的主键，固定为 1。
const LOCK_ID: i64 = 1;
/// 锁的租约时长。持有者异常退出后，其它实例最多等这么久就能接管。
const LOCK_LEASE: Duration = Duration::from_secs(600);
/// 等锁时的轮询间隔。
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

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

/// 迁移命令可能遇到的错误。
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// 等待执行锁超时。
    #[error("等待迁移执行锁超时（等了 {waited:?}，持有者 {owner}）")]
    LockTimeout { owner: String, waited: Duration },
    /// 数据库错误。
    #[error(transparent)]
    Db(#[from] DbErr),
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
    pub async fn up<F, A>(
        &self,
        db: &DatabaseConnection,
        steps: Option<u32>,
    ) -> Result<RunReport, MigrateError>
    where
        F: MigratorTrait,
        A: MigratorTrait,
    {
        let guard = self.acquire_lock(db).await?;
        let framework = self.run_group::<F>(db, "framework", steps).await?;
        let app = self.run_group::<A>(db, "app", steps).await?;
        drop(guard);
        Ok(RunReport { framework, app })
    }

    /// 只读查询两组迁移的状态：不建表、不加锁。
    pub async fn status<F, A>(db: &DatabaseConnection) -> Result<Vec<GroupStatus>, MigrateError>
    where
        F: MigratorTrait,
        A: MigratorTrait,
    {
        Ok(vec![
            Self::group_status::<F>(db, "framework").await?,
            Self::group_status::<A>(db, "app").await?,
        ])
    }

    async fn run_group<M: MigratorTrait>(
        &self,
        db: &DatabaseConnection,
        scope: &str,
        steps: Option<u32>,
    ) -> Result<GroupReport, MigrateError> {
        let pending_before = migration_names(M::get_pending_migrations(db).await?);
        let result = M::up(db, steps).await;
        // 失败时也记录已经写入跟踪表的迁移，方便定位。
        let applied = migration_names(M::get_applied_migrations(db).await?)
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

    async fn group_status<M: MigratorTrait>(
        db: &DatabaseConnection,
        scope: &'static str,
    ) -> Result<GroupStatus, MigrateError> {
        let applied = migration_names(M::get_applied_migrations_read_only(db).await?);
        let pending = migration_names(M::get_pending_migrations_read_only(db).await?);
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

    async fn acquire_lock(&self, db: &DatabaseConnection) -> Result<LockGuard, MigrateError> {
        ensure_lock_table(db).await?;
        let deadline = Instant::now() + self.lock_timeout;
        loop {
            let lease_until = unix_now() + LOCK_LEASE.as_secs() as i64;
            if try_acquire(db, &self.instance, lease_until).await? {
                return Ok(LockGuard {
                    db: db.clone(),
                    owner: self.instance.clone(),
                });
            }
            if Instant::now() >= deadline {
                let owner = lock_owner(db).await?.unwrap_or_default();
                return Err(MigrateError::LockTimeout {
                    owner,
                    waited: self.lock_timeout,
                });
            }
            tokio::time::sleep(LOCK_POLL_INTERVAL).await;
        }
    }
}

/// 持有执行锁的凭证，离开作用域时释放。
#[derive(Debug)]
pub struct LockGuard {
    db: DatabaseConnection,
    owner: String,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Drop 里不能 await，交给运行时在后台释放。
        let db = self.db.clone();
        let owner = self.owner.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = release_lock(&db, &owner).await;
            });
        }
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn ensure_lock_table(db: &DatabaseConnection) -> Result<(), DbErr> {
    let stmt = Table::create()
        .table(Alias::new(LOCK_TABLE))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("id"))
                .integer()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(Alias::new("owner")).string().not_null())
        .col(
            ColumnDef::new(Alias::new("lease_until"))
                .big_integer()
                .not_null(),
        )
        .to_owned();
    db.execute(&stmt).await?;
    Ok(())
}

/// 占用执行锁。锁空闲、归自己、或租约已过期时成功。
async fn try_acquire(
    db: &DatabaseConnection,
    owner: &str,
    lease_until: i64,
) -> Result<bool, DbErr> {
    let insert = Query::insert()
        .into_table(Alias::new(LOCK_TABLE))
        .columns([
            Alias::new("id"),
            Alias::new("owner"),
            Alias::new("lease_until"),
        ])
        .values_panic([LOCK_ID.into(), owner.to_owned().into(), lease_until.into()])
        .on_conflict(OnConflict::column(Alias::new("id")).do_nothing().to_owned())
        .to_owned();
    if db.execute(&insert).await?.rows_affected() > 0 {
        return Ok(true);
    }

    let update = Query::update()
        .table(Alias::new(LOCK_TABLE))
        .values([
            (Alias::new("owner"), Expr::val(owner.to_owned())),
            (Alias::new("lease_until"), Expr::val(lease_until)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(LOCK_ID))
        .and_where(
            Expr::col(Alias::new("owner"))
                .eq(owner.to_owned())
                .or(Expr::col(Alias::new("lease_until")).lt(unix_now())),
        )
        .to_owned();
    Ok(db.execute(&update).await?.rows_affected() > 0)
}

async fn release_lock(db: &DatabaseConnection, owner: &str) -> Result<(), DbErr> {
    let stmt = Query::delete()
        .from_table(Alias::new(LOCK_TABLE))
        .and_where(Expr::col(Alias::new("id")).eq(LOCK_ID))
        .and_where(Expr::col(Alias::new("owner")).eq(owner.to_owned()))
        .to_owned();
    db.execute(&stmt).await?;
    Ok(())
}

async fn lock_owner(db: &DatabaseConnection) -> Result<Option<String>, DbErr> {
    let stmt = Query::select()
        .column(Alias::new("owner"))
        .from(Alias::new(LOCK_TABLE))
        .and_where(Expr::col(Alias::new("id")).eq(LOCK_ID))
        .to_owned();
    let row = db.query_one(&stmt).await?;
    row.map(|row| row.try_get::<String>("", "owner"))
        .transpose()
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
