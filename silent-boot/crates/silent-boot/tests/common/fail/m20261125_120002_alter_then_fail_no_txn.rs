use super::m20261125_120001_alter_then_fail::alter_then_fail;
use silent_boot::db::migrate::*;

/// 和上一条做同样的事，但没有覆盖 `use_transaction`，用来对照事务带来的差别。
#[derive(Debug, DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        alter_then_fail(manager).await
    }
}
