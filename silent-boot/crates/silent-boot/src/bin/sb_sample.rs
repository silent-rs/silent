//! 验证用的小应用：入口交给 `silent_boot::run`，脚手架生成的 `main.rs` 就是这个样子。
//!
//! 它自己没有迁移，也没有业务路由，只用来验证子命令命中后执行并退出、不启动服务。

use std::process::ExitCode;

use silent_boot::db::migrate::{MigrationTrait, MigratorTrait};
use silent_boot::prelude::*;

/// 这个小应用自己没有迁移。
#[derive(Debug, Default, Clone, Copy)]
struct SampleMigrator;

impl MigratorTrait for SampleMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Vec::new()
    }
}

async fn serve(config: AppConfig) -> BootResult {
    let app = App::with_config(config)
        .migrator(SampleMigrator)
        .build()
        .await?;
    tracing::info!("sb-sample is up: it has no routes besides the system ones");
    app.run().await
}

#[tokio::main]
async fn main() -> ExitCode {
    silent_boot::run(SampleMigrator, serve).await
}
