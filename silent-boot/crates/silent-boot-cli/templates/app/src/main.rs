//! 应用的入口。第一个参数是 silent-boot 的子命令（`migrate`、`generate`、`healthcheck`）就执行它，
//! 否则读配置、连库、启动服务。

//! 应用的入口。第一个参数是 silent-boot 的子命令（`migrate`、`generate`、`healthcheck`）就执行它，
//! 否则读配置、连库、启动服务。能被测试用到的东西都在库 `{{ crate }}` 里。

use {{ crate }}::app;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    silent_boot::run(migration::Migrator, app::serve).await
}
