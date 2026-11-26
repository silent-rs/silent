use silent_boot::db::migrate::*;

silent_boot::migration! {
    /// skeleton
    pub struct Migration;

    async fn up(_manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
