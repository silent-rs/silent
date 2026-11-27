//! silent-boot：基于 Silent 的全栈快速开发框架。
//!
//! 模块随里程碑逐步补齐，当前有：
//! - [`app`]：启动器，读配置、连库、装配中间件和路由、优雅关停；
//! - [`config`]：约定配置，`config/app.toml` 加 `APP__` 前缀的环境变量；
//! - [`db`]：连接管理与迁移，迁移是数据库结构的唯一事实来源；
//! - [`web`]：统一响应与错误、带校验的提取器、分页、静态资源、运维端点；
//! - [`cli`]：应用二进制里的子命令，`migrate`、`healthcheck`、`generate`。
//!
//! 应用的入口只有一句 [`run`]。最常用的类型在 [`prelude`] 里。
//! 规划见仓库根目录的 `PLAN.md`，技术方案见 `docs/design.md`。
pub mod app;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod db;
pub mod logging;
pub mod prelude;
pub mod web;

#[cfg(feature = "cli")]
pub use cli::app_root;

/// 应用的入口：第一个参数是框架的子命令就执行它并退出，否则加载配置、初始化日志，调用 `serve` 启动服务。
///
/// ```no_run
/// use silent_boot::prelude::*;
///
/// async fn serve(config: AppConfig) -> BootResult {
///     let app = App::with_config(config)
///         .migrator(migration::Migrator)
///         .route(api::routes())
///         .build()
///         .await?;
///     app.run().await
/// }
///
/// #[tokio::main]
/// async fn main() -> std::process::ExitCode {
///     silent_boot::run(migration::Migrator, serve).await
/// }
/// ```
///
/// 启动失败时把原因打到标准错误，退出码是 1。帮助信息和用法错误由 clap 打印并退出，
/// 分别是 0 和 2。子命令的退出码见各自的说明，例如 [`cli::Migrate::Lint`] 不通过时是 1。
#[cfg(feature = "cli")]
pub async fn run<M, F, Fut>(migrator: M, serve: F) -> std::process::ExitCode
where
    M: db::migrate::MigratorTrait + Sync + 'static,
    F: FnOnce(config::AppConfig) -> Fut,
    Fut: std::future::Future<Output = Result<(), app::BootError>>,
{
    use std::process::ExitCode;

    let cli = match cli::parse_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    if let Some(cli) = cli {
        let ctx = cli::CommandContext::discover(migrator);
        let mut out = std::io::stdout().lock();
        return match cli::dispatch(cli, &ctx, &mut out).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                // 上面已经把检查的细节打到标准输出了，这里只补一句结论。
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        };
    }

    let config = match config::AppConfig::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("应用启动失败：{e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = logging::init(&config) {
        eprintln!("应用启动失败：{e}");
        return ExitCode::from(1);
    }
    match serve(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "应用启动失败");
            eprintln!("应用启动失败：{e}");
            ExitCode::from(1)
        }
    }
}
