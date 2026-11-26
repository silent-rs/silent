//! 验证用的小应用：演示子命令命中后不启动服务。M1 会换成脚手架模板里的 main.rs。
//!
//! 数据库地址取自环境变量 `SB_DATABASE_URL`。

use sea_orm::Database;
#[cfg(feature = "codegen")]
use silent_boot::cli::Generate;
use silent_boot::cli::{Command, Migrate};
use silent_boot::db::migrate::{FrameworkMigrator, MigrationTrait, MigratorTrait};

/// 这个小应用自己没有迁移。
struct SampleMigrator;

impl MigratorTrait for SampleMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Vec::new()
    }
}

#[tokio::main]
async fn main() {
    let Some(cli) = silent_boot::cli::parse() else {
        println!("no subcommand: the server would start here");
        return;
    };

    let url = match std::env::var("SB_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("SB_DATABASE_URL is not set");
            std::process::exit(2);
        }
    };
    let result = match cli.command {
        Command::Migrate { action } => {
            let db = Database::connect(&url).await.expect("connect");
            match action {
                Migrate::Up { steps, dry_run } => silent_boot::cli::migrate::up::<
                    FrameworkMigrator,
                    SampleMigrator,
                >(&db, steps, dry_run)
                .await
                .map_err(|e| e.to_string()),
                Migrate::Status { json } => silent_boot::cli::migrate::status::<
                    FrameworkMigrator,
                    SampleMigrator,
                >(&db, json)
                .await
                .map_err(|e| e.to_string()),
            }
        }
        #[cfg(feature = "codegen")]
        Command::Generate {
            action:
                Generate::Entity {
                    tables,
                    ignore_tables,
                    dry_run,
                },
        } => {
            let root = std::path::Path::new("entity/src");
            let options = silent_boot::cli::generate::EntityOptions {
                tables,
                ignore_tables,
                dry_run,
            };
            silent_boot::cli::generate::generate_entity(&url, root, &options)
                .await
                .map(|generated| {
                    println!(
                        "{} files in {}",
                        generated.files.len(),
                        generated.output_dir.display()
                    );
                    for file in &generated.changed {
                        println!(
                            "  {} {}",
                            if dry_run { "would change" } else { "changed" },
                            file.display()
                        );
                    }
                })
                .map_err(|e| e.to_string())
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
