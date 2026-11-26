//! `silent-boot` 全局工具：`new` 生成应用，`dev` 构建并运行应用、监听变更、重启、刷新页面。
//!
//! 其余子命令（`migrate`、`generate`、`healthcheck`）属于应用自己，在应用目录里用
//! `cargo run -- <子命令>` 执行，这样它们和应用用的是同一份依赖。

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use silent_boot_cli::dev::{CargoBuild, DevLoop, Startup};
use silent_boot_cli::events::Events;
use silent_boot_cli::new::{self, Database, DependencySource};
use silent_boot_cli::watch::SourceWatcher;

#[derive(Debug, Parser)]
#[command(name = "silent-boot", version, about = "silent-boot 的全局工具")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 生成一个应用的骨架：迁移、实体、分层的示例模块、接口测试、配置。
    New {
        /// 应用名，也是目录名和 Cargo 包名。
        #[arg(value_name = "NAME")]
        name: String,
        /// 用哪种数据库。
        #[arg(long, value_enum, default_value_t = Database::Sqlite)]
        db: Database,
        /// 顺带执行 git init。
        #[arg(long)]
        git: bool,
        /// 目录里已经有文件时，覆盖脚手架生成的文件。
        #[arg(long)]
        force: bool,
        /// 用本地目录里的 silent-boot（开发框架自己时用），而不是 crates.io 上的版本。
        #[arg(long, value_name = "DIR", requires = "path_silent")]
        path_framework: Option<PathBuf>,
        /// 和 `--path-framework` 配套：本地 silent crate 的目录。
        #[arg(long, value_name = "DIR")]
        path_silent: Option<PathBuf>,
    },
    /// 构建并运行应用，监听源码变更，自动重新构建、重启，并通知页面刷新。
    Dev {
        /// 要运行的可执行目标；不填则取当前包的第一个可执行目标。
        #[arg(long)]
        bin: Option<String>,
        /// 事件服务器的端口，页面通过它等待刷新通知。
        #[arg(long, default_value_t = 9230)]
        port: u16,
        /// 额外监听的目录或文件，逗号分隔。
        #[arg(long = "watch", value_delimiter = ',')]
        watch: Vec<PathBuf>,
        /// 启动前先执行一次 `migrate up`。默认不执行，有待执行的迁移时只提示。
        #[arg(long)]
        migrate: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::New {
            name,
            db,
            git,
            force,
            path_framework,
            path_silent,
        } => new_app(name, db, git, force, path_framework, path_silent),
        Command::Dev {
            bin,
            port,
            watch,
            migrate,
        } => dev(bin, port, watch, migrate).await,
    }
}

fn new_app(
    name: String,
    db: Database,
    git: bool,
    force: bool,
    path_framework: Option<PathBuf>,
    path_silent: Option<PathBuf>,
) -> anyhow::Result<()> {
    let target = PathBuf::from(&name);
    let deps = match (path_framework, path_silent) {
        (Some(silent_boot), Some(silent)) => DependencySource::Local {
            silent_boot,
            silent,
        },
        (None, None) => DependencySource::Published,
        _ => anyhow::bail!("--path-framework 和 --path-silent 要一起给"),
    };
    let options = new::Options {
        name: name.clone(),
        database: db,
        deps,
        target: target.clone(),
    };
    let files = new::scaffold(&options)?;
    let written = new::write_to(&target, &files, force)?;
    if git && !target.join(".git").exists() {
        let status = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&target)
            .status()?;
        if !status.success() {
            eprintln!("git init 没有成功，跳过");
        }
    }
    println!("created {} files in {}", written.len(), target.display());
    println!();
    println!("接下来：");
    println!("  cd {}", name);
    println!("  cargo run -- migrate up          # 建库、建表");
    println!("  cargo run                        # 启动，http://127.0.0.1:8080");
    println!("  silent-boot dev --migrate        # 改代码自动重启（需要安装 silent-boot-cli）");
    Ok(())
}

async fn dev(
    bin: Option<String>,
    port: u16,
    watch: Vec<PathBuf>,
    migrate: bool,
) -> anyhow::Result<()> {
    let app_dir = std::env::current_dir()?;
    let build = CargoBuild::new(app_dir.clone(), bin)
        .await
        .map_err(anyhow::Error::msg)?;
    println!("[dev] 运行 {}，目录 {}", build.bin(), app_dir.display());
    // 默认监听应用的源码、迁移、实体、模板和清单；不在的目录自动跳过。
    let mut dirs: Vec<PathBuf> = ["src", "entity/src", "migration/src", "templates"]
        .iter()
        .map(|dir| app_dir.join(dir))
        .collect();
    let mut files: Vec<PathBuf> = vec![app_dir.join("Cargo.toml")];
    for extra in watch {
        if extra.is_dir() {
            dirs.push(extra);
        } else {
            files.push(extra);
        }
    }
    dirs.retain(|dir| dir.exists());
    files.retain(|file| file.exists());
    let mut watcher = SourceWatcher::start(&dirs, &files)?;
    let events = Arc::new(Events::bind(([127, 0, 0, 1], port).into()).await?);
    let script_url = events.script_url();
    println!(
        "[dev] 开发构建的应用会自动把这个脚本注入 HTML，后端重启后页面自己刷新：{}",
        events.script_url()
    );
    // 迁移相关的子命令是应用自己的，只对 silent-boot 应用（目录里有 silent-boot.toml）使用。
    let startup = if app_dir.join("silent-boot.toml").is_file() {
        Startup {
            migrate,
            hint_pending: !migrate,
        }
    } else {
        Startup::NONE
    };
    DevLoop::new(build, app_dir, events)
        .envs(vec![("SB_DEV_RELOAD_URL".to_owned(), script_url)])
        .startup(startup)
        .run(watcher.changes(), async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    println!("[dev] 已停止");
    Ok(())
}
