//! `generate entity`：按库结构生成实体文件。
//!
//! 默认行为与基准命令一致：
//! `sea-orm-cli generate entity --with-serde both -v -o entity/src/ -l`。
//! 固定约定（输出 `lib.rs`、派生 serde、chrono 时间类型、保留空的行为实现、跳过跟踪表和系统表）
//! 不提供参数，见 `docs/design.md` §6.4。
//!
//! 目前只支持 SQLite。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sea_orm_codegen::{
    BannerVersion, BigIntegerType, DateTimeCrate, EntityFormat, EntityTransformer,
    EntityWriterContext, OutputFile, WithPrelude, WithSerde,
};
use sea_schema::sqlite::def::ColumnVisibility;
use sea_schema::sqlite::discovery::SchemaDiscovery;

/// 默认跳过的表：两张迁移跟踪表，以及框架自己的执行记录表和锁表。
pub const DEFAULT_IGNORED_TABLES: &[&str] = &[
    "seaql_migrations",
    "sb_migrations",
    "sb_migration_log",
    "sb_locks",
];

/// `generate entity` 的可选参数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityOptions {
    /// 只处理这些表；为空表示全部。
    pub tables: Vec<String>,
    /// 在默认跳过的表之外再跳过的表。
    pub ignore_tables: Vec<String>,
    /// 只列出会变化的文件，不写入。
    pub dry_run: bool,
}

/// 一次生成的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 写出（或将要写出）的文件，相对于输出目录。
    pub files: Vec<PathBuf>,
    /// 内容与磁盘上不同的文件，相对于输出目录。
    pub changed: Vec<PathBuf>,
}

/// 生成实体时的错误。
#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    /// 暂不支持的数据库地址。
    #[error("暂不支持这个数据库地址，目前只支持 sqlite：{0}")]
    UnsupportedUrl(String),
    /// 连接或读取库结构失败。
    #[error("连接或读取库结构失败：{0}")]
    Discovery(String),
    /// 没有匹配到任何表。
    ///
    /// 这种情况下 `sea-orm-codegen` 仍会产出空的 `lib.rs` 和 `prelude.rs`，落盘就会把已有的索引文件清空，
    /// 所以直接报错。
    #[error("没有匹配到任何表，没有写入文件")]
    NoTables,
    /// 生成实体或格式化失败。
    #[error("生成实体失败：{0}")]
    Codegen(String),
    /// 写文件失败。
    #[error("写文件失败：{0}")]
    Io(#[from] std::io::Error),
}

/// 按库结构生成实体到 `output_dir`。
///
/// 输出的文件名由 `sea-orm-codegen` 决定，内容先经 rustfmt 格式化再落盘，和 sea-orm-cli 的做法相同。
pub async fn generate_entity(
    database_url: &str,
    output_dir: &Path,
    options: &EntityOptions,
) -> Result<Generated, GenerateError> {
    if !database_url.starts_with("sqlite:") {
        return Err(GenerateError::UnsupportedUrl(database_url.to_owned()));
    }
    let pool = sqlx::SqlitePool::connect(database_url)
        .await
        .map_err(|e| GenerateError::Discovery(e.to_string()))?;
    let schema = SchemaDiscovery::new(pool)
        .discover()
        .await
        .map_err(|e| GenerateError::Discovery(e.to_string()))?
        .merge_indexes_into_table();

    let table_stmts = schema
        .tables
        .into_iter()
        .filter(|table| options.tables.is_empty() || options.tables.contains(&table.name))
        .filter(|table| !DEFAULT_IGNORED_TABLES.contains(&table.name.as_str()))
        .filter(|table| !options.ignore_tables.contains(&table.name))
        .map(|mut table| {
            // 生成列不能当作普通字段写回，sea-orm-cli 也跳过它们。
            table.columns.retain(|col| {
                !matches!(
                    col.hidden,
                    ColumnVisibility::GeneratedVirtual | ColumnVisibility::GeneratedStored
                )
            });
            table.write()
        })
        .collect::<Vec<sea_orm::sea_query::TableCreateStatement>>();
    // 空着生成会把已有的 lib.rs 和 prelude.rs 清空，所以没有表就报错。
    if table_stmts.is_empty() {
        return Err(GenerateError::NoTables);
    }

    let context = EntityWriterContext::new(
        EntityFormat::default(),
        WithPrelude::default(),
        WithSerde::Both,
        false,
        DateTimeCrate::Chrono,
        BigIntegerType::default(),
        None,
        // 索引文件用 lib.rs
        true,
        false,
        false,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
        // 保留文件末尾那行空的 ActiveModelBehavior 实现
        true,
        BannerVersion::default(),
    );
    let output = EntityTransformer::transform(table_stmts)
        .map_err(|e| GenerateError::Codegen(e.to_string()))?
        .generate(&context);

    let mut generated = Generated {
        output_dir: output_dir.to_path_buf(),
        files: Vec::new(),
        changed: Vec::new(),
    };
    for OutputFile { name, content } in output.files.iter() {
        let path = output_dir.join(name);
        let content = rustfmt(content)?;
        let unchanged = fs::read_to_string(&path).is_ok_and(|existing| existing == content);
        generated.files.push(PathBuf::from(name));
        if unchanged {
            continue;
        }
        generated.changed.push(PathBuf::from(name));
        if options.dry_run {
            continue;
        }
        fs::create_dir_all(output_dir)?;
        fs::write(&path, content)?;
    }
    Ok(generated)
}

/// 用 rustfmt 的默认配置格式化，等价于 sea-orm-cli 写完文件后对它执行 `rustfmt`。
fn rustfmt(source: &str) -> Result<String, GenerateError> {
    let mut child = Command::new("rustfmt")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GenerateError::Codegen(format!("启动 rustfmt 失败：{e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(source.as_bytes())
            .map_err(|e| GenerateError::Codegen(format!("写入 rustfmt 失败：{e}")))?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(GenerateError::Codegen(format!(
            "rustfmt 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| GenerateError::Codegen(format!("rustfmt 的输出不是 UTF-8：{e}")))
}
