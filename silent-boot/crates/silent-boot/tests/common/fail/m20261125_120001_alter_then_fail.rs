use silent_boot::db::migrate::*;

/// 给 article 表加一列 `extra`，然后返回错误。
pub async fn alter_then_fail(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Alias::new("article"))
                .add_column(ColumnDef::new(Alias::new("extra")).string())
                .to_owned(),
        )
        .await?;
    Err(DbErr::Custom(
        "deliberate failure after ALTER TABLE".to_owned(),
    ))
}

silent_boot::migration! {
    /// 加列之后故意失败，用来验证事务回滚。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        alter_then_fail(manager).await
    }
}
