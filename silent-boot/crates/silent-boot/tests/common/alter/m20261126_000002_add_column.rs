use silent_boot::db::migrate::*;

silent_boot::migration! {
    /// 给 article 表加一列。
    pub struct Migration;

    async fn up(manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("article"))
                    .add_column(ColumnDef::new(Alias::new("extra")).string())
                    .to_owned(),
            )
            .await
    }
}
