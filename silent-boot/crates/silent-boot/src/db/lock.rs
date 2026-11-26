//! 具名执行锁：用一张表里的租约行实现，SQLite、MySQL、PostgreSQL 的写法相同。
//!
//! 迁移执行器用它保证同一时刻只有一个实例在迁移（锁名 `migrate`）；调度器之后用它
//! 保证同一个任务不会被两个实例同时执行（锁名 `job:<id>`）。
//!
//! 锁靠租约而不是连接：持有者崩溃后，其它实例最多等一个租约时长就能接管。

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// `ExprTrait` 提供 `eq`、`lt` 这些比较方法，要显式引入。
use sea_orm::sea_query::{Alias, ColumnDef, Expr, ExprTrait, OnConflict, Query, Table};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// 存放所有具名锁的表。
pub const LOCK_TABLE: &str = "sb_locks";

/// 等锁的参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockOptions {
    /// 租约时长。持有者异常退出后，其它实例最多等这么久就能接管。
    pub lease: Duration,
    /// 拿不到锁时，隔多久再试一次。
    pub poll_interval: Duration,
    /// 最长等待多久。
    pub timeout: Duration,
}

impl Default for LockOptions {
    fn default() -> Self {
        Self {
            lease: Duration::from_secs(600),
            poll_interval: Duration::from_millis(100),
            timeout: Duration::from_secs(30),
        }
    }
}

/// 拿锁时的错误。
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// 等到超时也没拿到。
    #[error("等待锁 {name} 超时（等了 {waited:?}，持有者 {owner}）")]
    Timeout {
        /// 锁名。
        name: String,
        /// 超时那一刻的持有者。
        owner: String,
        /// 实际等待的时长上限。
        waited: Duration,
    },
    /// 数据库错误。
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// 持有锁的凭证。离开作用域时释放；拿得准的路径上用 [`LockGuard::release`] 显式释放。
#[derive(Debug)]
pub struct LockGuard {
    db: DatabaseConnection,
    name: String,
    owner: String,
    released: bool,
}

impl LockGuard {
    /// 释放锁，等释放完成才返回。
    ///
    /// 命令执行完进程可能马上退出：那时 Drop 里排给运行时的后台任务不一定来得及跑，
    /// 锁要等到租约过期才会被别人拿到。所以 `MigrationRunner::up` 在返回前显式调用它。
    pub async fn release(mut self) {
        self.released = true;
        let _ = release(&self.db, &self.name, &self.owner).await;
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        // Drop 里不能 await，交给运行时在后台释放。
        let db = self.db.clone();
        let name = self.name.clone();
        let owner = self.owner.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = release(&db, &name, &owner).await;
            });
        }
    }
}

/// 获取名为 `name` 的锁。拿不到就按 `poll_interval` 重试，直到 `timeout`。
///
/// 锁空闲、归自己、或租约已过期时都能拿到，所以同一个 `owner` 可以重入。
pub async fn acquire(
    db: &DatabaseConnection,
    name: &str,
    owner: &str,
    options: &LockOptions,
) -> Result<LockGuard, LockError> {
    ensure_table(db).await?;
    let deadline = Instant::now() + options.timeout;
    loop {
        let lease_until = unix_now() + options.lease.as_secs() as i64;
        if try_acquire(db, name, owner, lease_until).await? {
            return Ok(LockGuard {
                db: db.clone(),
                name: name.to_owned(),
                owner: owner.to_owned(),
                released: false,
            });
        }
        if Instant::now() >= deadline {
            let holder = holder(db, name).await?.unwrap_or_default();
            return Err(LockError::Timeout {
                name: name.to_owned(),
                owner: holder,
                waited: options.timeout,
            });
        }
        tokio::time::sleep(options.poll_interval).await;
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn ensure_table(db: &DatabaseConnection) -> Result<(), DbErr> {
    let stmt = Table::create()
        .table(Alias::new(LOCK_TABLE))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("name"))
                .string()
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

/// 占用一次。锁空闲、归自己、或租约已过期时成功。
async fn try_acquire(
    db: &DatabaseConnection,
    name: &str,
    owner: &str,
    lease_until: i64,
) -> Result<bool, DbErr> {
    let insert = Query::insert()
        .into_table(Alias::new(LOCK_TABLE))
        .columns([
            Alias::new("name"),
            Alias::new("owner"),
            Alias::new("lease_until"),
        ])
        .values_panic([
            name.to_owned().into(),
            owner.to_owned().into(),
            lease_until.into(),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("name"))
                .do_nothing()
                .to_owned(),
        )
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
        .and_where(Expr::col(Alias::new("name")).eq(name.to_owned()))
        .and_where(
            Expr::col(Alias::new("owner"))
                .eq(owner.to_owned())
                .or(Expr::col(Alias::new("lease_until")).lt(unix_now())),
        )
        .to_owned();
    Ok(db.execute(&update).await?.rows_affected() > 0)
}

async fn release(db: &DatabaseConnection, name: &str, owner: &str) -> Result<(), DbErr> {
    let stmt = Query::delete()
        .from_table(Alias::new(LOCK_TABLE))
        .and_where(Expr::col(Alias::new("name")).eq(name.to_owned()))
        .and_where(Expr::col(Alias::new("owner")).eq(owner.to_owned()))
        .to_owned();
    db.execute(&stmt).await?;
    Ok(())
}

async fn holder(db: &DatabaseConnection, name: &str) -> Result<Option<String>, DbErr> {
    let stmt = Query::select()
        .column(Alias::new("owner"))
        .from(Alias::new(LOCK_TABLE))
        .and_where(Expr::col(Alias::new("name")).eq(name.to_owned()))
        .to_owned();
    let row = db.query_one(&stmt).await?;
    row.map(|row| row.try_get::<String>("", "owner"))
        .transpose()
}
