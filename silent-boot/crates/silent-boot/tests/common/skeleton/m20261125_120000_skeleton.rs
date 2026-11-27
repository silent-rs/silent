use silent_boot::db::migrate::*;

silent_boot::migration! {
    /// skeleton
    ///
    /// 要能用 `migrate down` 回退的话，在 `up` 后面补一个
    /// `async fn down(manager: &SchemaManager) -> Result<(), DbErr>`；没写的迁移回退时会报错。
    pub struct Migration;

    async fn up(_manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
