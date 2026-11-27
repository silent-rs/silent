use silent_boot::db::migrate::*;

silent_boot::migration! {
    /// 把 article 表的 extra 列改名为 extra_renamed。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("article"))
                    .add_column(ColumnDef::new(Alias::new("extra")).string())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("article"))
                    .rename_column(Alias::new("extra"), Alias::new("extra_renamed"))
                    .to_owned(),
            )
            .await
    }
}
