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

/// 建 article 表。列与审计字段按 `docs/design.md` §6.11 的约定。
pub async fn create_article(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Article::Table)
                .col(
                    ColumnDef::new(Article::Id)
                        .string()
                        .not_null()
                        .primary_key(),
                )
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

silent_boot::migration! {
    /// 创建 article 表。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        create_article(manager).await
    }

    async fn down(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Article::Table).to_owned())
            .await
    }
}
