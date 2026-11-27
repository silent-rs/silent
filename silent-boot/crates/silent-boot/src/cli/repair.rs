//! `migrate baseline` 和 `migrate repair`：直接改迁移记录，不运行迁移。
//!
//! - [`baseline`]：接入已有数据库。库里的结构已经是现状，把应用的待执行迁移标记成已执行，
//!   之后的新变更才走迁移。框架的迁移不由它处理，照常由 `migrate up` 执行。
//! - [`mark_applied`]、[`mark_pending`]：迁移执行到一半、人工修好之后收尾。例如 MySQL 的 DDL
//!   不能回滚，一条迁移只做了一半：修完数据后把它标记成已执行，或者撤销记录后重跑。

// `ExprTrait` 提供 `eq` 这些比较方法，要显式引入。
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Query};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

use crate::db::migrate::Migrator;

/// 改记录失败。
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct RepairError(#[from] DbErr);

/// 一次 `baseline` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineReport {
    /// 被改写的跟踪表。
    pub table: String,
    /// 被标记成已执行的迁移，按执行顺序。
    pub marked: Vec<String>,
}

/// 把应用当前的待执行迁移全部标记成已执行。跟踪表不存在时先建表。
pub async fn baseline(
    db: &DatabaseConnection,
    migrator: &dyn Migrator,
) -> Result<BaselineReport, RepairError> {
    migrator.install(db).await?;
    let pending = migrator.diff(db).await?.pending;
    let table = migrator.table_name();
    for version in &pending {
        insert(db, &table, version).await?;
    }
    Ok(BaselineReport {
        table,
        marked: pending,
    })
}

/// 把一条迁移标记成已执行。已经是已执行时返回 `false`。
pub async fn mark_applied(
    db: &DatabaseConnection,
    migrator: &dyn Migrator,
    version: &str,
) -> Result<bool, RepairError> {
    migrator.install(db).await?;
    if migrator.recorded(db).await?.iter().any(|v| v == version) {
        return Ok(false);
    }
    insert(db, &migrator.table_name(), version).await?;
    Ok(true)
}

/// 把一条迁移的记录撤掉，之后 `migrate up` 会重跑它。没有这条记录时返回 `false`。
pub async fn mark_pending(
    db: &DatabaseConnection,
    migrator: &dyn Migrator,
    version: &str,
) -> Result<bool, RepairError> {
    let stmt = Query::delete()
        .from_table(Alias::new(migrator.table_name()))
        .and_where(Expr::col(Alias::new("version")).eq(version))
        .to_owned();
    let result = db.execute(&stmt).await?;
    Ok(result.rows_affected() > 0)
}

/// 写一行跟踪记录。`applied_at` 和 SeaORM 自己写的一样，是 Unix 秒。
async fn insert(db: &DatabaseConnection, table: &str, version: &str) -> Result<(), RepairError> {
    let stmt = Query::insert()
        .into_table(Alias::new(table))
        .columns([Alias::new("version"), Alias::new("applied_at")])
        .values_panic([
            version.to_owned().into(),
            chrono::Utc::now().timestamp().into(),
        ])
        .to_owned();
    db.execute(&stmt).await?;
    Ok(())
}
