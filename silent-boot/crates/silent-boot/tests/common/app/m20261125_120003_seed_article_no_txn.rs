use super::m20261125_120001_seed_article::seed_article;
use silent_boot::db::migrate::*;

/// 和 `m20261125_120001_seed_article` 做同样的事，但不覆盖 `use_transaction`，用来对照。
#[derive(Debug, DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        super::seed_delay().await;
        seed_article(manager.get_connection(), 2).await
    }
}
