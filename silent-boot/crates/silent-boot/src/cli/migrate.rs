//! `migrate` 子命令：生成迁移文件、执行迁移、打印结果。
//!
//! 数据库连接由应用提供（来自应用配置），所以这里不读环境变量。
//! `F` 是框架迁移器，`A` 是应用自己的迁移器。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::db::migrate::{GroupStatus, MigrateError, MigrationRunner, Migrator};
use sea_orm::DatabaseConnection;

/// 迁移器 `src/lib.rs` 里登记 `mod` 声明的受管区块起点。
pub const MODS_BEGIN: &str = "// <silent-boot:gen:mods>";
/// 登记 `mod` 声明的受管区块终点。
pub const MODS_END: &str = "// </silent-boot:gen:mods>";
/// 迁移器 `src/lib.rs` 里登记迁移列表的受管区块起点。
pub const LIST_BEGIN: &str = "// <silent-boot:gen:list>";
/// 登记迁移列表的受管区块终点。
pub const LIST_END: &str = "// </silent-boot:gen:list>";

/// 一次 `migrate generate` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMigration {
    /// 迁移的模块名，也是文件名，形如 `m20261126_093000_create_article`。
    pub module: String,
    /// 生成的迁移文件。
    pub file: PathBuf,
}

/// 生成迁移文件时的错误。
#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    /// 迁移名称为空或含有连字符。
    #[error("迁移名称不能为空，也不能含连字符：{0:?}")]
    InvalidName(String),
    /// 迁移器的 `src/lib.rs` 里找不到受管区块的标记。
    #[error("迁移器的 src/lib.rs 里找不到标记 {0}。脚手架生成的 lib.rs 自带这些标记")]
    MissingMarkers(String),
    /// 同名的迁移文件已经存在。
    #[error("迁移文件已存在：{0}")]
    AlreadyExists(PathBuf),
    /// 读写文件失败。
    #[error("读写文件失败：{0}")]
    Io(#[from] io::Error),
}

/// 生成一条迁移的文件，并登记到迁移器的受管区块里。
///
/// `migration_crate` 是迁移 crate 的根目录，里面有 `src/lib.rs`。
/// `at` 决定文件名里的时间戳；文件名规则与 sea-orm-cli 相同：`m%Y%m%d_%H%M%S_<名称>`。
///
/// 生成的文件里是一条空的 `up`，在事务里执行。登记时按模块名排序，
/// 所以时间早的迁移排在前面，先执行。
pub fn generate(
    name: &str,
    at: chrono::NaiveDateTime,
    migration_crate: &Path,
) -> Result<NewMigration, GenerateError> {
    let name = name.trim();
    if name.is_empty() || name.contains('-') {
        return Err(GenerateError::InvalidName(name.to_owned()));
    }
    let snake = name.split_whitespace().collect::<Vec<_>>().join("_");
    let module = format!("m{}_{}", at.format("%Y%m%d_%H%M%S"), snake);

    let src = migration_crate.join("src");
    let file = src.join(format!("{module}.rs"));
    if file.exists() {
        return Err(GenerateError::AlreadyExists(file));
    }
    // 先确认能登记，再写文件，免得登记失败时留下一条没人引用的迁移。
    let lib = src.join("lib.rs");
    let registered = registered(&fs::read_to_string(&lib)?, &module)?;

    fs::create_dir_all(&src)?;
    fs::write(&file, skeleton(name))?;
    fs::write(&lib, registered)?;
    Ok(NewMigration { module, file })
}

/// 新迁移文件的内容。`tests/common/skeleton/` 里有一份同样的文件，保证它能编译。
fn skeleton(description: &str) -> String {
    format!(
        "use silent_boot::db::migrate::*;\n\
         \n\
         silent_boot::migration! {{\n\
         \x20   /// {description}\n\
         \x20   pub struct Migration;\n\
         \n\
         \x20   async fn up(_manager: &SchemaManager) -> Result<(), DbErr> {{\n\
         \x20       Ok(())\n\
         \x20   }}\n\
         }}\n"
    )
}

/// 返回登记了 `module` 之后的迁移器 `lib.rs` 内容：两个受管区块里各多一行，区块内按名称排序。
fn registered(source: &str, module: &str) -> Result<String, GenerateError> {
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    insert_sorted(
        &mut lines,
        MODS_BEGIN,
        MODS_END,
        &format!("pub mod {module};"),
    )?;
    insert_sorted(
        &mut lines,
        LIST_BEGIN,
        LIST_END,
        &format!("Box::new({module}::Migration),"),
    )?;
    let mut content = lines.join("\n");
    content.push('\n');
    Ok(content)
}

/// 在 `begin` 和 `end` 两行之间登记一条，保持区块内按名称排序，缩进沿用起点那一行。
fn insert_sorted(
    lines: &mut Vec<String>,
    begin: &str,
    end: &str,
    entry: &str,
) -> Result<(), GenerateError> {
    let start =
        line_index(lines, begin).ok_or_else(|| GenerateError::MissingMarkers(begin.to_owned()))?;
    let stop =
        line_index(lines, end).ok_or_else(|| GenerateError::MissingMarkers(end.to_owned()))?;
    if stop <= start {
        return Err(GenerateError::MissingMarkers(end.to_owned()));
    }
    let indent = lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    let mut entries: Vec<String> = lines[start + 1..stop]
        .iter()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    if !entries.iter().any(|existing| existing == entry) {
        entries.push(entry.to_owned());
        entries.sort();
    }
    let formatted: Vec<String> = entries
        .iter()
        .map(|existing| format!("{indent}{existing}"))
        .collect();
    lines.splice(start + 1..stop, formatted);
    Ok(())
}

fn line_index(lines: &[String], marker: &str) -> Option<usize> {
    lines.iter().position(|line| line.trim() == marker)
}

/// `migrate up`：加锁后先执行框架迁移，再执行应用迁移。结果写到 `out`。
pub async fn up<W: io::Write>(
    db: &DatabaseConnection,
    framework: &dyn Migrator,
    app: &dyn Migrator,
    steps: Option<u32>,
    dry_run: bool,
    out: &mut W,
) -> Result<(), MigrateError> {
    if dry_run {
        let groups = MigrationRunner::status(db, framework, app).await?;
        print_groups(&groups, false, out)?;
        return Ok(());
    }
    let report = MigrationRunner::default()
        .up(db, framework, app, steps)
        .await?;
    for (scope, applied) in [
        ("framework", &report.framework.applied),
        ("app", &report.app.applied),
    ] {
        writeln!(out, "{scope}: {} applied", applied.len())?;
        for name in applied {
            writeln!(out, "  {name}")?;
        }
    }
    Ok(())
}

/// `migrate status`：只读查询，不建表、不加锁。结果写到 `out`。
pub async fn status<W: io::Write>(
    db: &DatabaseConnection,
    framework: &dyn Migrator,
    app: &dyn Migrator,
    json: bool,
    out: &mut W,
) -> Result<(), MigrateError> {
    let groups = MigrationRunner::status(db, framework, app).await?;
    if json {
        // `GroupStatus` 只有字符串和字符串数组，序列化不会失败。
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&groups).expect("status is serializable")
        )?;
        return Ok(());
    }
    print_groups(&groups, true, out)?;
    Ok(())
}

fn print_groups<W: io::Write>(
    groups: &[GroupStatus],
    show_applied: bool,
    out: &mut W,
) -> Result<(), MigrateError> {
    for group in groups {
        writeln!(out, "{}", group.scope)?;
        if show_applied {
            for name in &group.applied {
                writeln!(out, "  applied  {name}")?;
            }
        }
        for name in &group.pending {
            writeln!(out, "  pending  {name}")?;
        }
        if group.pending.is_empty() && (!show_applied || group.applied.is_empty()) {
            writeln!(out, "  up to date")?;
        }
    }
    Ok(())
}
