//! 创建 article 表。示例模块用的表，列按 silent-boot 的审计字段约定：
//! `id`、`created_at`、`updated_at`、`created_by`、`updated_by`、`deleted_at`。
//!
//! 审计时间不给数据库默认值（数据库按自己的时区取值），由 service 层写入。

use silent_boot::db::migrate::*;

#[derive(DeriveIden)]
pub enum Article {
    Table,
    Id,
    Title,
    Content,
    Status,
    CreatedBy,
    UpdatedBy,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

silent_boot::migration! {
    /// 创建 article 表。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Article::Table)
                    .col(ColumnDef::new(Article::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Article::Title).string().not_null())
                    .col(ColumnDef::new(Article::Content).text())
                    .col(
                        ColumnDef::new(Article::Status)
                            .tiny_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Article::CreatedBy).string())
                    .col(ColumnDef::new(Article::UpdatedBy).string())
                    .col(ColumnDef::new(Article::CreatedAt).date_time().not_null())
                    .col(ColumnDef::new(Article::UpdatedAt).date_time().not_null())
                    .col(ColumnDef::new(Article::DeletedAt).date_time())
                    .to_owned(),
            )
            .await
    }

    // 回退：删表。开发时用 `migrate down`，或者 `silent-boot dev` 面板里的 `d`。
    async fn down(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Article::Table).to_owned())
            .await
    }
}
