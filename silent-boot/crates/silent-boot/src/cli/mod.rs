//! 命令行子命令：由应用二进制在入口处分派，见 `docs/design.md` §6.4 与 §13。
//!
//! ```no_run
//! if let Some(cli) = silent_boot::cli::parse() {
//!     // 命中子命令：执行它，不再启动服务。具体做法见 `migrate` 和 `generate` 两个模块。
//!     println!("{cli:?}");
//! } else {
//!     // 第一个参数不是框架的子命令，照常启动服务
//! }
//! ```
//!
//! 帮助信息、版本号和用法错误会打印到终端并退出进程；第一个参数不是框架认识的子命令时，
//! [`parse`] 返回 `None`，由应用启动服务。
#[cfg(feature = "codegen")]
pub mod generate;
pub mod migrate;

use std::ffi::OsString;

use clap::{Parser, Subcommand};

/// 框架认识的顶层参数。第一个参数不在其中时，应用照常启动服务。
///
/// 启用了 `codegen` feature 时还包括 `generate`，见 [`is_top_level`]。
const TOP_LEVEL: &[&str] = &["migrate", "help", "--help", "-h", "-V", "--version"];

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
    },
    /// 列出已执行和待执行的迁移。
    Status {
        /// 以 JSON 输出。
        #[arg(long)]
        json: bool,
    },
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
