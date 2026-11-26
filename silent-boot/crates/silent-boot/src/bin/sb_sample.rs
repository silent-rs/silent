//! 验证用的小应用：演示子命令命中后不启动服务。M1 会换成脚手架模板里的 main.rs。
//!
//! 数据库地址取自环境变量 `SB_DATABASE_URL`，只有需要连库的子命令才会读它。

use sea_orm::{Database, DatabaseConnection};
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

    let result = match cli.command {
        Command::Migrate { action } => match action {
            Migrate::Generate { name } => {
                let created = silent_boot::cli::migrate::generate(
                    &name,
                    chrono::Local::now().naive_local(),
                    std::path::Path::new("migration"),
                );
                match created {
                    Ok(created) => {
                        println!("created {}", created.file.display());
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            Migrate::Up { steps, dry_run } => {
                let db = connect().await;
                silent_boot::cli::migrate::up::<FrameworkMigrator, SampleMigrator>(
                    &db, steps, dry_run,
                )
                .await
                .map_err(|e| e.to_string())
            }
            Migrate::Status { json } => {
                let db = connect().await;
                silent_boot::cli::migrate::status::<FrameworkMigrator, SampleMigrator>(&db, json)
                    .await
                    .map_err(|e| e.to_string())
            }
        },
        #[cfg(feature = "codegen")]
        Command::Generate {
            action:
                Generate::Entity {
                    tables,
                    ignore_tables,
                    dry_run,
                },
        } => {
            let url = database_url();
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

fn database_url() -> String {
    match std::env::var("SB_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("SB_DATABASE_URL is not set");
            std::process::exit(2);
        }
    }
}

async fn connect() -> DatabaseConnection {
    Database::connect(database_url()).await.expect("connect")
}
