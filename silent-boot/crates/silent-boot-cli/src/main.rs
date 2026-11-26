//! `silent-boot` 全局工具。现在只有 `dev`；其余子命令随里程碑补上。
//!
//! 其余子命令（`migrate`、`generate`）属于应用自己，在应用目录里用 `cargo run -- <子命令>` 执行，
//! 这样它们和应用用的是同一份依赖。全局工具之后会把这些子命令转发过去。

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use silent_boot_cli::dev::{CargoBuild, DevLoop};
use silent_boot_cli::events::Events;
use silent_boot_cli::watch::SourceWatcher;

#[derive(Debug, Parser)]
#[command(name = "silent-boot", version, about = "silent-boot 的全局工具")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        command: Command::Dev { bin, port, watch },
    } = Cli::parse();

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
    println!(
        "[dev] 页面里引入这个脚本，后端重启后就会自动刷新：{}",
        events.script_url()
    );

    DevLoop::new(build, app_dir, events)
        .run(watcher.changes(), async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    println!("[dev] 已停止");
    Ok(())
}
