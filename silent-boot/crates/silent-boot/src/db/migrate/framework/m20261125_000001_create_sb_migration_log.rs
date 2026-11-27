//! 框架迁移：迁移执行的审计表。

use crate::db::migrate::*;

#[derive(DeriveIden)]
enum SbMigrationLog {
    Table,
    Id,
    Scope,
    Instance,
    PendingBefore,
    AppliedAfter,
    Status,
    StartedAt,
    FinishedAt,
    Error,
}

crate::migration! {
    /// 创建 `sb_migration_log`：每次执行迁移，按框架、应用各记一行。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SbMigrationLog::Table)
                    .col(
                        ColumnDef::new(SbMigrationLog::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SbMigrationLog::Scope).string().not_null())
                    .col(ColumnDef::new(SbMigrationLog::Instance).string().not_null())
                    .col(ColumnDef::new(SbMigrationLog::PendingBefore).text().not_null())
                    .col(ColumnDef::new(SbMigrationLog::AppliedAfter).text().not_null())
                    .col(ColumnDef::new(SbMigrationLog::Status).string().not_null())
                    .col(ColumnDef::new(SbMigrationLog::StartedAt).date_time().not_null())
                    .col(ColumnDef::new(SbMigrationLog::FinishedAt).date_time())
                    .col(ColumnDef::new(SbMigrationLog::Error).text())
                    .to_owned(),
            )
            .await
    }
}
