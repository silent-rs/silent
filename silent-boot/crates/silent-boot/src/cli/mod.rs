//! 命令行子命令：由应用二进制在入口处分派，见 `docs/design.md` §6.4 与 §13。
//!
//! 应用的 `main.rs` 把入口交给 [`crate::run`]：第一个参数是框架的子命令就执行它并退出，
//! 否则启动服务。各个子命令的实现见 [`migrate`]、[`healthcheck`]、[`lint`]、[`repair`]，
//! 启用 `codegen` feature 时还有 [`generate`]。
//!
//! 帮助信息、版本号和用法错误会打印到终端并退出进程；第一个参数不是框架认识的子命令时，
//! [`parse_from`] 返回 `None`，由应用启动服务。

pub mod healthcheck;
pub mod lint;
pub mod migrate;
pub mod repair;

#[cfg(feature = "codegen")]
pub mod generate;
#[cfg(feature = "codegen")]
pub mod module;

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use sea_orm::DatabaseConnection;

use crate::config::{AppConfig, ConfigError};
use crate::db::connect::{self, ConnectError, ConnectOptions};
use crate::db::migrate::{AnyMigrator, MigrateError};

/// 标记应用根目录的文件。脚手架会生成它；在子目录里执行命令时向上找这个文件。
pub const APP_FILE: &str = "silent-boot.toml";

/// 环境变量形式的数据库地址，优先级低于 `--database-url`，高于配置文件。
pub const DATABASE_URL_ENV: &str = "SB_DATABASE_URL";

/// 框架认识的顶层参数。第一个参数不在其中时，应用照常启动服务。
///
/// 启用了 `codegen` feature 时还包括 `generate`，见 [`is_top_level`]。
const TOP_LEVEL: &[&str] = &[
    "migrate",
    "healthcheck",
    "help",
    "--help",
    "-h",
    "-V",
    "--version",
];

fn is_top_level(arg: &str) -> bool {
    #[cfg(feature = "codegen")]
    if arg == "generate" {
        return true;
    }
    TOP_LEVEL.contains(&arg)
}

/// 命令行。
#[derive(Debug, Parser)]
#[command(
    name = "silent-boot",
    version,
    about = "silent-boot 的命令行子命令",
    propagate_version = true
)]
pub struct Cli {
    /// 要执行的子命令。
    #[command(subcommand)]
    pub command: Command,
}

/// 顶层子命令。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 执行迁移、查看状态。
    Migrate {
        /// 迁移相关的动作。
        #[command(subcommand)]
        action: Migrate,
    },
    /// 探测正在运行的服务是否健康，给容器的健康检查用。健康时退出码是 0。
    Healthcheck {
        /// 探测地址，例如 `http://127.0.0.1:8080/healthz`。不写时取配置里的 `server.listen`。
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        /// 多久没回应算失败，例如 `3s`。默认 3 秒。
        #[arg(long, value_name = "DURATION")]
        timeout: Option<String>,
    },
    /// 生成代码。只在启用 `codegen` feature 时存在。
    #[cfg(feature = "codegen")]
    Generate {
        /// 生成相关的动作。
        #[command(subcommand)]
        action: Generate,
    },
}

/// `migrate` 的动作。
#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Migrate {
    /// 生成一条迁移的文件并登记到迁移器：`migrate generate "create article"`。
    Generate {
        /// 迁移名称，例如 "create article"。会变成文件名的一部分，空格换成下划线。
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// 执行待执行的迁移：先框架，后应用。
    Up {
        /// 每组最多执行几条。
        #[arg(long)]
        steps: Option<u32>,
        /// 只列出待执行的迁移，不执行。
        #[arg(long)]
        dry_run: bool,
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
    /// 列出已执行和待执行的迁移。
    Status {
        /// 以 JSON 输出。
        #[arg(long)]
        json: bool,
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
    /// 检查迁移文件：破坏性操作要带 `// silent-boot: destructive` 标记，文件要登记进迁移器。
    /// 有问题时退出码是 1，可以直接放进 CI。
    Lint {
        /// 只检查相对这个 Git 引用新增的迁移文件，例如 `origin/main`。不写时检查全部。
        #[arg(long, value_name = "GIT_REF")]
        since: Option<String>,
    },
    /// 接入已有数据库：把应用当前的待执行迁移直接标记成已执行，不实际运行。
    /// 框架迁移照常执行，之后再运行 `migrate up` 即可。
    Baseline {
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
    /// 手工改迁移记录：把一条迁移标记成已执行或未执行。给迁移执行到一半、人工修复之后收尾用。
    Repair {
        /// 把这条迁移标记成已执行。
        #[arg(long, value_name = "VERSION", conflicts_with = "pending")]
        applied: Option<String>,
        /// 把这条迁移标记成未执行。
        #[arg(long, value_name = "VERSION")]
        pending: Option<String>,
        /// 改哪一组迁移的记录。
        #[arg(long, value_enum, default_value_t = Scope::App)]
        scope: Scope,
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
}

/// 两组迁移：应用自己的，和框架内置的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    /// 应用的迁移，记录在 `seaql_migrations`。
    App,
    /// 框架的迁移，记录在 `sb_migrations`。
    Framework,
}

/// `generate` 的动作。
#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Generate {
    /// 按库结构生成实体文件到 `entity/src/`。
    Entity {
        /// 只处理这些表，逗号分隔。
        #[arg(long = "tables", value_delimiter = ',')]
        tables: Vec<String>,
        /// 在默认跳过的表之外再跳过的表，逗号分隔。
        #[arg(long = "ignore-tables", value_delimiter = ',')]
        ignore_tables: Vec<String>,
        /// 只列出会变化的文件，不写入。
        #[arg(long)]
        dry_run: bool,
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
    /// 按一张表生成 dto、service、api 三个文件，并登记到 `src/api/mod.rs` 等文件里。
    /// 前提：迁移已经执行，实体已经生成。
    Module {
        /// 按哪张表生成，也是模块名。
        #[arg(long = "table", value_name = "TABLE")]
        table: String,
        /// 已有的文件照样覆盖。默认不覆盖：生成出来的代码归应用所有。
        #[arg(long)]
        force: bool,
        /// 只列出会写出、会变化的文件，不写入。
        #[arg(long)]
        dry_run: bool,
        /// 覆盖配置里的数据库地址。
        #[arg(long, value_name = "URL")]
        database_url: Option<String>,
    },
}

/// 读取进程参数并解析。
///
/// 返回 `None` 表示第一个参数不是框架的子命令，应用应当照常启动服务。
pub fn parse() -> Option<Cli> {
    match parse_from(std::env::args_os()) {
        Ok(cli) => cli,
        // 帮助和版本信息走标准输出并以 0 退出，用法错误走标准错误并以 2 退出。
        Err(e) => e.exit(),
    }
}

/// [`parse`] 的可测试版本：不读环境变量，也不退出进程。
pub fn parse_from<I, T>(args: I) -> Result<Option<Cli>, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match args.get(1).and_then(|arg| arg.to_str()) {
        Some(first) if is_top_level(first) => Cli::try_parse_from(args).map(Some),
        _ => Ok(None),
    }
}

/// 应用根目录：从当前目录向上找 [`APP_FILE`]，找不到就用当前目录。
pub fn app_root() -> PathBuf {
    let Ok(cwd) = std::env::current_dir() else {
        return PathBuf::from(".");
    };
    for dir in cwd.ancestors() {
        if dir.join(APP_FILE).is_file() {
            return dir.to_path_buf();
        }
    }
    cwd
}

/// 执行子命令需要的东西：应用根目录、环境变量、两组迁移器。
pub struct CommandContext {
    /// 应用根目录。
    pub root: PathBuf,
    /// 框架的迁移器。
    pub framework: AnyMigrator,
    /// 应用的迁移器。
    pub app: AnyMigrator,
    vars: Vec<(String, String)>,
}

impl CommandContext {
    /// 应用根目录由 [`app_root`] 确定，环境变量取自进程环境。
    pub fn discover(app: impl Into<AnyMigrator>) -> Self {
        Self::with_root(app_root(), app)
    }

    /// 指定应用根目录，环境变量取自进程环境。
    pub fn with_root(root: impl Into<PathBuf>, app: impl Into<AnyMigrator>) -> Self {
        Self::with_env(root, app, std::env::vars())
    }

    /// 指定应用根目录和环境变量，给测试用。
    pub fn with_env<I>(root: impl Into<PathBuf>, app: impl Into<AnyMigrator>, vars: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        Self {
            root: root.into(),
            framework: AnyMigrator::framework(),
            app: app.into(),
            vars: vars.into_iter().collect(),
        }
    }

    /// 按应用根目录下的 `config/` 加载配置，再叠上环境变量。
    pub fn config(&self) -> Result<AppConfig, CommandError> {
        Ok(AppConfig::load_from(&self.root, self.vars.clone())?)
    }

    /// 建立默认连接。地址的优先级：命令行的 `--database-url`、环境变量 `SB_DATABASE_URL`、配置文件。
    pub async fn connect(
        &self,
        override_url: Option<&str>,
    ) -> Result<DatabaseConnection, CommandError> {
        let mut config = self.config()?;
        let url = override_url
            .map(str::to_owned)
            .or_else(|| {
                self.vars
                    .iter()
                    .find(|(key, _)| key == DATABASE_URL_ENV)
                    .map(|(_, value)| value.clone())
            })
            .unwrap_or_else(|| config.database.url.clone());
        config.database.url = url;
        config.validate()?;
        let production = config.app.env.is_production();
        Ok(connect::connect(&ConnectOptions::from_config(&config.database, production)).await?)
    }

    /// 某一组迁移的迁移器。
    pub fn migrator(&self, scope: Scope) -> &AnyMigrator {
        match scope {
            Scope::App => &self.app,
            Scope::Framework => &self.framework,
        }
    }
}

/// 子命令失败的原因。打印给使用者，所以每一条都是一句完整的话。
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// 配置不对。
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// 连不上数据库。
    #[error(transparent)]
    Connect(#[from] ConnectError),
    /// 迁移执行失败。
    #[error(transparent)]
    Migrate(#[from] MigrateError),
    /// 读写文件失败。
    #[error(transparent)]
    Io(#[from] io::Error),
    /// 命令做了检查，结果不通过；或者参数的取值不可用。
    #[error("{0}")]
    Failed(String),
}

/// 执行一条子命令，输出写到 `out`。需要连库的子命令才加载配置、才连库。
pub async fn dispatch<W: io::Write>(
    cli: Cli,
    ctx: &CommandContext,
    out: &mut W,
) -> Result<(), CommandError> {
    match cli.command {
        Command::Migrate { action } => match action {
            Migrate::Generate { name } => {
                let created = migrate::generate(
                    &name,
                    chrono::Local::now().naive_local(),
                    &ctx.root.join("migration"),
                )?;
                writeln!(out, "created {}", created.file.display())?;
            }
            Migrate::Up {
                steps,
                dry_run,
                database_url,
            } => {
                let db = ctx.connect(database_url.as_deref()).await?;
                migrate::up(
                    &db,
                    ctx.framework.as_dyn(),
                    ctx.app.as_dyn(),
                    steps,
                    dry_run,
                    out,
                )
                .await?;
            }
            Migrate::Status { json, database_url } => {
                let db = ctx.connect(database_url.as_deref()).await?;
                migrate::status(&db, ctx.framework.as_dyn(), ctx.app.as_dyn(), json, out).await?;
            }
            Migrate::Lint { since } => {
                let added = match &since {
                    Some(git_ref) => Some(lint::added_since(&ctx.root, git_ref)?),
                    None => None,
                };
                let report = lint::lint(&ctx.root.join("migration"), added.as_deref())?;
                report.write_to(out)?;
                if !report.findings.is_empty() {
                    return Err(CommandError::Failed(format!(
                        "migrate lint 没通过：{} 个问题",
                        report.findings.len()
                    )));
                }
            }
            Migrate::Baseline { database_url } => {
                let db = ctx.connect(database_url.as_deref()).await?;
                let report = repair::baseline(&db, ctx.app.as_dyn()).await?;
                writeln!(
                    out,
                    "baseline: {} migration(s) marked as applied in {}",
                    report.marked.len(),
                    report.table
                )?;
                for name in &report.marked {
                    writeln!(out, "  {name}")?;
                }
                if report.marked.is_empty() {
                    writeln!(out, "  nothing to do")?;
                }
                writeln!(
                    out,
                    "next: run `migrate up` to apply the framework migrations"
                )?;
            }
            Migrate::Repair {
                applied,
                pending,
                scope,
                database_url,
            } => {
                let (version, clear) = match (applied, pending) {
                    (Some(version), _) => (version, false),
                    (None, Some(version)) => (version, true),
                    (None, None) => {
                        return Err(CommandError::Failed(
                            "migrate repair 需要 --applied <版本> 或 --pending <版本>".to_owned(),
                        ));
                    }
                };
                let db = ctx.connect(database_url.as_deref()).await?;
                let migrator = ctx.migrator(scope).as_dyn();
                let changed = if clear {
                    repair::mark_pending(&db, migrator, &version).await?
                } else {
                    repair::mark_applied(&db, migrator, &version).await?
                };
                let state = if clear { "pending" } else { "applied" };
                if changed {
                    writeln!(out, "{version} is now marked as {state} ({scope:?})")?;
                } else {
                    writeln!(out, "{version} was already {state} ({scope:?})")?;
                }
            }
        },
        Command::Healthcheck { url, timeout } => {
            let timeout = match timeout.as_deref() {
                Some(text) => crate::config::Duration::parse(text)
                    .map_err(|e| CommandError::Failed(e.to_string()))?
                    .into(),
                None => std::time::Duration::from_secs(3),
            };
            let (addr, path) = match url.as_deref() {
                Some(url) => {
                    healthcheck::parse_url(url).map_err(|e| CommandError::Failed(e.to_string()))?
                }
                None => {
                    let listen = self_listen(&ctx.config()?).ok_or_else(|| {
                        CommandError::Failed("配置里的 server.listen 不是可以探测的地址".to_owned())
                    })?;
                    (listen, "/healthz".to_owned())
                }
            };
            let status = healthcheck::check(addr, &path, timeout).await?;
            if status.is_success() {
                writeln!(out, "healthy: {addr}{path} returned {}", status.as_u16())?;
            } else {
                return Err(CommandError::Failed(format!(
                    "unhealthy: {addr}{path} returned {}",
                    status.as_u16()
                )));
            }
        }
        #[cfg(feature = "codegen")]
        Command::Generate {
            action:
                Generate::Entity {
                    tables,
                    ignore_tables,
                    dry_run,
                    database_url,
                },
        } => {
            let url = match database_url {
                Some(url) => url,
                None => ctx.config()?.database.url,
            };
            let root = ctx.root.join("entity/src");
            let options = generate::EntityOptions {
                tables,
                ignore_tables,
                dry_run,
            };
            let generated = generate::generate_entity(&url, &root, &options).await?;
            writeln!(
                out,
                "{} files in {}",
                generated.files.len(),
                generated.output_dir.display()
            )?;
            for file in &generated.changed {
                writeln!(
                    out,
                    "  {} {}",
                    if dry_run { "would change" } else { "changed" },
                    file.display()
                )?;
            }
        }
        #[cfg(feature = "codegen")]
        Command::Generate {
            action:
                Generate::Module {
                    table,
                    force,
                    dry_run,
                    database_url,
                },
        } => {
            let url = match database_url {
                Some(url) => url,
                None => ctx.config()?.database.url,
            };
            let options = module::ModuleOptions {
                table,
                force,
                dry_run,
            };
            let generated = module::generate_module(&url, &ctx.root, &options).await?;
            writeln!(
                out,
                "module {}: {} file(s) {}",
                generated.module,
                generated.files.len(),
                if dry_run { "to write" } else { "written" },
            )?;
            for file in &generated.files {
                writeln!(
                    out,
                    "  {} {}",
                    if dry_run { "would write" } else { "wrote" },
                    file.display()
                )?;
            }
            for file in &generated.skipped {
                writeln!(
                    out,
                    "  kept      {} (already exists; use --force to overwrite)",
                    file.display()
                )?;
            }
        }
    }
    Ok(())
}

/// 配置里的监听地址换成本机可连的地址：`0.0.0.0` 和 `[::]` 换成 `127.0.0.1`。
fn self_listen(config: &AppConfig) -> Option<std::net::SocketAddr> {
    let mut addr: std::net::SocketAddr = config.server.listen.parse().ok()?;
    if addr.ip().is_unspecified() {
        addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    }
    Some(addr)
}

/// 让调用方不必显式写 `From<GenerateError>`。
impl From<migrate::GenerateError> for CommandError {
    fn from(err: migrate::GenerateError) -> Self {
        match err {
            migrate::GenerateError::Io(e) => CommandError::Io(e),
            other => CommandError::Failed(other.to_string()),
        }
    }
}

#[cfg(feature = "codegen")]
impl From<module::ModuleError> for CommandError {
    fn from(err: module::ModuleError) -> Self {
        match err {
            module::ModuleError::Io(e) => CommandError::Io(e),
            other => CommandError::Failed(other.to_string()),
        }
    }
}

#[cfg(feature = "codegen")]
impl From<generate::GenerateError> for CommandError {
    fn from(err: generate::GenerateError) -> Self {
        match err {
            generate::GenerateError::Io(e) => CommandError::Io(e),
            other => CommandError::Failed(other.to_string()),
        }
    }
}

impl From<healthcheck::HealthError> for CommandError {
    fn from(err: healthcheck::HealthError) -> Self {
        CommandError::Failed(err.to_string())
    }
}

impl From<lint::LintError> for CommandError {
    fn from(err: lint::LintError) -> Self {
        CommandError::Failed(err.to_string())
    }
}

impl From<repair::RepairError> for CommandError {
    fn from(err: repair::RepairError) -> Self {
        CommandError::Failed(err.to_string())
    }
}
