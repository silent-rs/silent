//! 按需手动运行的小工具，不进 CI。
mod common;

use common::app::AppMigrator;
use silent_boot::db::migrate::{FrameworkMigrator, MigrationRunner};

/// 用样例迁移生成一个数据库文件，供 sea-orm-cli 生成实体使用。
///
/// ```text
/// SB_SAMPLE_DB=/tmp/sb-m0-sample/article.db \
///     cargo test -p silent-boot --test tools write_sample_db -- --ignored
/// ```
#[tokio::test]
#[ignore = "手动工具"]
async fn write_sample_db() {
    let path = std::env::var("SB_SAMPLE_DB").expect("set SB_SAMPLE_DB");
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{path}?mode=rwc");
    let db = sea_orm::Database::connect(&url).await.expect("connect");
    let report = MigrationRunner::new("sample")
        .up(&db, &FrameworkMigrator, &AppMigrator, None)
        .await
        .expect("migrate");
    println!("applied {} migrations to {path}", report.applied_total());
}
