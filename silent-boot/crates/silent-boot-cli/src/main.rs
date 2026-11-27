//! `silent-boot` 全局工具。
//!
//! - `new`：生成应用的骨架。
//! - `dev`：构建并运行应用，监听源码变更，自动重新构建、重启，并通知页面刷新。
//!   在终端里是分栏面板：左边重启、执行迁移、回退迁移，右边日志（`--plain` 不用面板）。
//! - `migrate`、`generate`、`healthcheck`：在应用目录里转发给应用自己的子命令（`cargo run -- <子命令>`），
//!   这样它们和应用用的是同一份依赖，迁移列表也是应用自己的。参数原样交过去，`--help` 也能用。
//!
//! 以上三种之外的命令都属于应用自己的二进制，在应用目录里用 `cargo run -- <子命令>` 执行。

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, Subcommand};
use silent_boot_cli::dev::{CargoBuild, DevCommand, DevLoop, Reporter, Startup};
use silent_boot_cli::events::Events;
use silent_boot_cli::forward::{self, APP_FILE, Forward};
use silent_boot_cli::new::{self, Database, DependencySource};
use silent_boot_cli::tui::{self, Screen};
use silent_boot_cli::watch::SourceWatcher;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Parser)]
#[command(name = "silent-boot", version, about = "silent-boot 的全局工具")]
struct Cli {
    /// 应用有多个可执行目标时，转发给哪一个。要写在子命令前面。
    #[arg(long, global = false, value_name = "NAME")]
    bin: Option<String>,
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
    /// 在终端里是分栏面板：左边重启、迁移这些操作，右边日志。
    Dev {
        /// 不用分栏面板，直接打印日志。标准输入输出不是终端时本来就是这样。
        #[arg(long)]
        plain: bool,
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
    /// 在应用里执行 `migrate …`：generate、up、status、lint、baseline、repair。参数原样交给应用。
    #[command(disable_help_flag = true)]
    Migrate {
        /// 交给应用的参数，例如 `up --steps 1`、`generate "create order"`。
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        args: Vec<OsString>,
    },
    /// 在应用里执行 `generate …`：entity、module。会自动打开应用的 `codegen` feature。
    #[command(disable_help_flag = true)]
    Generate {
        /// 交给应用的参数，例如 `entity`、`module --table article`。
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        args: Vec<OsString>,
    },
    /// 在应用里执行 `healthcheck …`：探测正在运行的服务。
    #[command(disable_help_flag = true)]
    Healthcheck {
        /// 交给应用的参数，例如 `--url http://127.0.0.1:8080/healthz`。
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        args: Vec<OsString>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::New {
            name,
            db,
            git,
            force,
            path_framework,
            path_silent,
        } => new_app(name, db, git, force, path_framework, path_silent),
        Command::Dev {
            plain,
            bin,
            port,
            watch,
            migrate,
        } => dev(bin, port, watch, migrate, plain).await,
        Command::Migrate { args } => forward_to_app(cli.bin, "migrate", args).await,
        Command::Generate { args } => forward_to_app(cli.bin, "generate", args).await,
        Command::Healthcheck { args } => forward_to_app(cli.bin, "healthcheck", args).await,
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// 把子命令交给应用：在当前目录往上找应用根目录，执行 `cargo run --quiet -- <子命令> <参数>`。
async fn forward_to_app(
    bin: Option<String>,
    subcommand: &'static str,
    args: Vec<OsString>,
) -> anyhow::Result<ExitCode> {
    let cwd = std::env::current_dir()?;
    let forward = Forward {
        subcommand,
        args: &args,
        bin: bin.as_deref(),
    };
    Ok(forward::forward(&cwd, &forward).await?)
}

fn new_app(
    name: String,
    db: Database,
    git: bool,
    force: bool,
    path_framework: Option<PathBuf>,
    path_silent: Option<PathBuf>,
) -> anyhow::Result<ExitCode> {
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
    println!(
        "  silent-boot migrate up            # 建库、建表（没有全局工具时用 cargo run -- migrate up）"
    );
    println!("  cargo run                        # 启动，http://127.0.0.1:8080");
    println!("  silent-boot dev --migrate        # 改代码自动重启");
    Ok(ExitCode::SUCCESS)
}

async fn dev(
    bin: Option<String>,
    port: u16,
    watch: Vec<PathBuf>,
    migrate: bool,
    plain: bool,
) -> anyhow::Result<ExitCode> {
    let app_dir = std::env::current_dir()?;
    let build = CargoBuild::new(app_dir.clone(), bin)
        .await
        .map_err(anyhow::Error::msg)?;
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
    let polling = watcher.is_polling();
    let events = Arc::new(Events::bind(([127, 0, 0, 1], port).into()).await?);
    let script_url = events.script_url();
    // 迁移相关的子命令是应用自己的，只对 silent-boot 应用（目录里有 silent-boot.toml）使用。
    let is_app = app_dir.join(APP_FILE).is_file();
    let startup = if is_app {
        Startup {
            migrate,
            hint_pending: !migrate,
        }
    } else {
        Startup::NONE
    };
    let envs = vec![("SB_DEV_RELOAD_URL".to_owned(), script_url.clone())];
    let interactive = !plain && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    if !interactive {
        println!("[dev] 运行 {}，目录 {}", build.bin(), app_dir.display());
        println!(
            "[dev] 开发构建的应用会自动把这个脚本注入 HTML，后端重启后页面自己刷新：{script_url}"
        );
        DevLoop::new(build, app_dir, events)
            .envs(envs)
            .startup(startup)
            .migrate_commands(is_app)
            .run(watcher.changes(), async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await;
        println!("[dev] 已停止");
        return Ok(ExitCode::SUCCESS);
    }

    let (redraw_tx, redraw_rx) = mpsc::channel(1);
    let screen = Arc::new(Screen::new(is_app, script_url.clone(), redraw_tx));
    screen.note(format!(
        "[dev] 运行 {}，目录 {}",
        build.bin(),
        app_dir.display()
    ));
    if polling {
        screen.note("[dev] 系统的文件事件监听不可用，改为每 500 毫秒轮询".to_owned());
    }
    screen.note(format!(
        "[dev] 页面刷新脚本 {script_url}，开发构建的应用会把它注入 HTML"
    ));

    let (commands_tx, commands_rx) = mpsc::channel::<DevCommand>(16);
    let (term_tx, term_rx) = mpsc::channel(16);
    let stop_reader = Arc::new(AtomicBool::new(false));
    tui::spawn_event_reader(term_tx, stop_reader.clone());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let looper = tokio::spawn({
        let events = events.clone();
        let screen = screen.clone();
        async move {
            DevLoop::with_reporter(build, app_dir, events, screen)
                .envs(envs)
                .startup(startup)
                .migrate_commands(is_app)
                .run_with_commands(watcher.changes(), commands_rx, async {
                    let _ = shutdown_rx.await;
                })
                .await;
        }
    });
    // 终端发来的 SIGTERM（比如容器要停了）也得能退出面板，不能只靠按键。
    let (interrupt_tx, interrupt_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(_) => std::future::pending::<()>().await,
            }
        }
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
        let _ = interrupt_tx.send(());
    });
    let panel = tui::run(screen, commands_tx, term_rx, redraw_rx, interrupt_rx).await;
    // 面板退出了：让循环停掉应用，再等它收尾。
    let _ = shutdown_tx.send(());
    let _ = looper.await;
    stop_reader.store(true, Ordering::SeqCst);
    ratatui::restore();
    panel?;
    println!("[dev] 已停止");
    Ok(ExitCode::SUCCESS)
}
