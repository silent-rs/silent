//! `migrate lint`：在写迁移的时候就拦下危险写法，见 `docs/design.md` §6.8。
//!
//! 检查两件事：
//!
//! - 迁移文件里有删表、删列、改名、清空、改列这类破坏性操作时，文件里必须有一行
//!   `// silent-boot: destructive`，说明是有意为之；
//! - `migration/src/` 下的迁移文件都登记进了迁移器的受管区块，登记的文件也都存在。
//!
//! 检查的是源码文本，不编译：所以字符串里的原生 SQL 也能查到，注释里的写法会被忽略。

use std::io;
use std::path::{Path, PathBuf};

use crate::cli::migrate::{LIST_BEGIN, LIST_END, MODS_BEGIN, MODS_END};

/// 破坏性操作的标记。带着它的文件不做破坏性操作的检查。
pub const DESTRUCTIVE_MARKER: &str = "// silent-boot: destructive";

/// 读不了文件，或者 git 执行失败。
#[derive(Debug, thiserror::Error)]
pub enum LintError {
    /// 读不了文件或目录。
    #[error("读取 {} 失败：{source}", path.display())]
    Io {
        /// 出问题的路径。
        path: PathBuf,
        /// 文件系统的错误。
        source: io::Error,
    },
    /// 用 git 找不到相对某个引用新增的文件。
    #[error("在 {} 里用 git 找相对 {git_ref} 新增的迁移文件失败：{stderr}", root.display())]
    Git {
        /// 应用根目录。
        root: PathBuf,
        /// 传进来的 Git 引用。
        git_ref: String,
        /// git 的错误输出。
        stderr: String,
    },
}

/// 一个问题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 迁移 crate 的 `src/lib.rs`，或者出问题的迁移文件。
    pub file: PathBuf,
    /// 行号，从 1 起。
    pub line: usize,
    /// 问题的代号：`drop-table`、`drop-column`、`rename-table`、`rename-column`、`truncate`、
    /// `modify-column`、`drop-index`、`unregistered`、`missing-file`。
    pub code: &'static str,
    /// 给人看的说明和处理办法。
    pub advice: String,
}

/// 检查结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// 检查了多少个迁移文件。
    pub checked: usize,
    /// 发现的问题，按文件、行号排序。
    pub findings: Vec<Finding>,
}

impl LintReport {
    /// 每个问题一行，最后一行是汇总。
    pub fn write_to<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        for finding in &self.findings {
            writeln!(
                out,
                "{}:{}: {}: {}",
                finding.file.display(),
                finding.line,
                finding.code,
                finding.advice
            )?;
        }
        if self.findings.is_empty() {
            writeln!(
                out,
                "migrate lint: 检查了 {} 个迁移文件，没有问题",
                self.checked
            )?;
        } else {
            writeln!(
                out,
                "migrate lint: 检查了 {} 个迁移文件，发现 {} 个问题",
                self.checked,
                self.findings.len()
            )?;
        }
        Ok(())
    }
}

/// 一条规则：源码里（小写化、去掉注释之后）出现 `pattern` 就是 `code` 这一类操作。
const RULES: &[(&str, &str)] = &[
    ("table::drop()", "drop-table"),
    (".drop_table(", "drop-table"),
    ("drop table ", "drop-table"),
    (".drop_column(", "drop-column"),
    ("drop column ", "drop-column"),
    ("drop index ", "drop-index"),
    ("table::rename()", "rename-table"),
    (".rename_table(", "rename-table"),
    ("rename to ", "rename-table"),
    (".rename_column(", "rename-column"),
    ("rename column ", "rename-column"),
    ("table::truncate()", "truncate"),
    (".truncate(", "truncate"),
    ("truncate ", "truncate"),
    (".modify_column(", "modify-column"),
    ("alter column ", "modify-column"),
];

fn describe(code: &'static str) -> &'static str {
    match code {
        "drop-table" => "删表",
        "drop-column" => "删列",
        "drop-index" => "删索引",
        "rename-table" => "给表改名",
        "rename-column" => "给列改名",
        "truncate" => "清空表",
        "modify-column" => "改列的类型或约束",
        _ => "破坏性操作",
    }
}

/// 检查 `migration_dir`（应用的迁移 crate）。
///
/// `added` 给出时只检查这些迁移文件，也不做登记检查：配合 `--since` 用在 CI 里，只看这次新增的迁移。
pub fn lint(migration_dir: &Path, added: Option<&[PathBuf]>) -> Result<LintReport, LintError> {
    let src = migration_dir.join("src");
    let mut files = Vec::new();
    match std::fs::read_dir(&src) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|source| LintError::Io {
                    path: src.clone(),
                    source,
                })?;
                let path = entry.path();
                let is_migration = path.extension().is_some_and(|ext| ext == "rs")
                    && path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .is_some_and(|stem| stem.starts_with('m'));
                if is_migration {
                    files.push(path);
                }
            }
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(LintError::Io {
                path: src.clone(),
                source: io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "没有这个目录，{} 不是应用的迁移 crate",
                        migration_dir.display()
                    ),
                ),
            });
        }
        Err(source) => {
            return Err(LintError::Io {
                path: src.clone(),
                source,
            });
        }
    }
    files.sort();

    let mut report = LintReport::default();
    let mut checked_names = Vec::new();
    for file in &files {
        if let Some(added) = added
            && !added.iter().any(|path| same_file(path, file))
        {
            continue;
        }
        let source = std::fs::read_to_string(file).map_err(|source| LintError::Io {
            path: file.clone(),
            source,
        })?;
        report.checked += 1;
        let name = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        checked_names.push(name);
        if source.contains(DESTRUCTIVE_MARKER) {
            continue;
        }
        for (index, line) in source.lines().enumerate() {
            let code = strip_comment(line).to_ascii_lowercase();
            if let Some((_, code)) = RULES.iter().find(|(pattern, _)| code.contains(pattern)) {
                report.findings.push(Finding {
                    file: file.clone(),
                    line: index + 1,
                    code,
                    advice: format!(
                        "{}。确认数据已经备份、旧版本还能跑之后，在这个文件里加一行 `{DESTRUCTIVE_MARKER}`；\
                         破坏性变更分两次发布，先加新结构并双写，下一个版本再删旧结构",
                        describe(code)
                    ),
                });
            }
        }
    }

    if added.is_none() {
        let lib = src.join("lib.rs");
        if let Ok(source) = std::fs::read_to_string(&lib) {
            report
                .findings
                .extend(registration_findings(&lib, &source, &checked_names));
        }
    }
    Ok(report)
}

/// 登记检查：受管区块里的 `mod` 声明和迁移列表，对上 `migration/src/` 下的迁移文件。
fn registration_findings(lib: &Path, source: &str, migration_names: &[String]) -> Vec<Finding> {
    #[derive(Clone, Copy)]
    enum Section {
        Mods,
        List,
    }
    let mut mods = Vec::new();
    let mut list = Vec::new();
    let mut section = None;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == MODS_BEGIN {
            section = Some(Section::Mods);
            continue;
        }
        if trimmed == LIST_BEGIN {
            section = Some(Section::List);
            continue;
        }
        if trimmed == MODS_END || trimmed == LIST_END {
            section = None;
            continue;
        }
        let (target, prefix, suffix): (&mut Vec<(String, usize)>, &str, &str) = match section {
            Some(Section::Mods) => (&mut mods, "pub mod ", ";"),
            Some(Section::List) => (&mut list, "Box::new(", "::Migration),"),
            None => continue,
        };
        if let Some(name) = trimmed
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
        {
            target.push((name.to_owned(), index + 1));
        }
    }

    let mut findings = Vec::new();
    for (name, line) in &mods {
        if !migration_names.contains(name) {
            findings.push(Finding {
                file: lib.to_path_buf(),
                line: *line,
                code: "missing-file",
                advice: format!("迁移器登记了 {name}，但 migration/src/{name}.rs 不存在"),
            });
        }
    }
    for name in migration_names {
        let in_mods = mods.iter().any(|(m, _)| m == name);
        let in_list = list.iter().any(|(m, _)| m == name);
        if in_mods && in_list {
            continue;
        }
        let how = match (in_mods, in_list) {
            (false, _) => {
                format!("在受管区块里补上 `pub mod {name};` 和迁移列表里的一项，或者删掉这个文件")
            }
            (true, false) => {
                format!("迁移列表的受管区块里缺少 `Box::new({name}::Migration),`")
            }
            _ => unreachable!("handled above"),
        };
        findings.push(Finding {
            file: lib.to_path_buf(),
            line: 0,
            code: "unregistered",
            advice: format!("迁移文件 {name}.rs 没有完整登记进迁移器：{how}"),
        });
    }
    findings
}

/// 去掉行内注释。字符串里的 `//`（例如地址）会被误伤，迁移里很少见。
fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || a.file_name() == b.file_name()
}

/// 相对 `git_ref` 新增的迁移文件（`migration/src` 下），路径是绝对路径。
///
/// 除了已经提交或暂存的新文件，还没加进 git 的文件也算：这样在提交之前本地就能跑。
pub fn added_since(root: &Path, git_ref: &str) -> Result<Vec<PathBuf>, LintError> {
    let mut files = git_lines(
        root,
        &[
            "diff",
            "--name-only",
            "--diff-filter=A",
            git_ref,
            "--",
            "migration/src",
        ],
        git_ref,
    )?;
    files.extend(git_lines(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            "migration/src",
        ],
        git_ref,
    )?);
    files.sort();
    files.dedup();
    Ok(files)
}

fn git_lines(root: &Path, args: &[&str], git_ref: &str) -> Result<Vec<PathBuf>, LintError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|source| LintError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(LintError::Git {
            root: root.to_path_buf(),
            git_ref: git_ref.to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| root.join(line.trim()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("migration/src");
        std::fs::create_dir_all(&src).unwrap();
        let migration = dir.path().join("migration");
        (dir, migration)
    }

    const LIB: &str = "\
// <silent-boot:gen:mods>
pub mod m20261126_000001_create_article;
pub mod m20261126_000002_drop_note;
// </silent-boot:gen:mods>
// <silent-boot:gen:list>
Box::new(m20261126_000001_create_article::Migration),
Box::new(m20261126_000002_drop_note::Migration),
// </silent-boot:gen:list>
";

    #[test]
    fn destructive_operations_need_the_marker() {
        let (_dir, migration) = workspace();
        write(
            &migration.join("src"),
            "lib.rs",
            "// <silent-boot:gen:mods>\npub mod m20261126_000001_drop_note;\n// </silent-boot:gen:mods>\n\
             // <silent-boot:gen:list>\nBox::new(m20261126_000001_drop_note::Migration),\n// </silent-boot:gen:list>\n",
        );
        write(
            &migration.join("src"),
            "m20261126_000001_drop_note.rs",
            "// 这里只是说明：Table::drop() 不算\n\
             let _ = manager.get_connection().execute_unprepared(\"DROP TABLE note\").await?;\n\
             Table::alter().table(A::Note).drop_column(A::Content).to_owned()\n",
        );
        let report = lint(&migration, None).unwrap();
        let codes = report
            .findings
            .iter()
            .map(|f| (f.line, f.code))
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec![(2, "drop-table"), (3, "drop-column")],
            "{report:#?}"
        );
        assert!(report.findings[0].advice.contains(DESTRUCTIVE_MARKER));
        assert_eq!(report.checked, 1);
    }

    #[test]
    fn marked_files_and_added_only_runs() {
        let (_dir, migration) = workspace();
        let src = migration.join("src");
        write(&src, "lib.rs", LIB);
        let drop_note = write(
            &src,
            "m20261126_000002_drop_note.rs",
            "// silent-boot: destructive\nTable::drop().table(A::Note).to_owned()\n",
        );
        write(
            &src,
            "m20261126_000001_create_article.rs",
            "Table::create().table(A::Article).to_owned()\n",
        );
        let report = lint(&migration, None).unwrap();
        assert_eq!(report.checked, 2);
        assert!(report.findings.is_empty(), "{report:#?}");

        // 新增的文件没有标记时才报；只检查指定文件时不做登记检查。
        std::fs::write(&drop_note, "Table::drop().table(A::Note).to_owned()\n").unwrap();
        let report = lint(&migration, Some(std::slice::from_ref(&drop_note))).unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.findings[0].code, "drop-table");
    }

    #[test]
    fn every_rule_has_a_description() {
        for (_, code) in RULES {
            assert_ne!(describe(code), "破坏性操作", "{code}");
        }
        assert!(
            RULES
                .iter()
                .all(|(pattern, _)| *pattern == pattern.to_lowercase())
        );
    }

    #[test]
    fn registration_is_checked_both_ways() {
        let (_dir, migration) = workspace();
        let src = migration.join("src");
        std::fs::write(
            src.join("lib.rs"),
            "// <silent-boot:gen:mods>\npub mod m20261126_000001_create_article;\n\
             pub mod m20261126_000003_ghost;\n// </silent-boot:gen:mods>\n\
             // <silent-boot:gen:list>\nBox::new(m20261126_000001_create_article::Migration),\n\
             // </silent-boot:gen:list>\n",
        )
        .unwrap();
        write(&src, "m20261126_000001_create_article.rs", "fn up() {}\n");
        write(&src, "m20261126_000002_add_index.rs", "fn up() {}\n");
        write(&src, "helper.rs", "fn up() {}\n");
        let report = lint(&migration, None).unwrap();
        let codes = report
            .findings
            .iter()
            .map(|f| (f.line, f.code, f.advice.clone()))
            .collect::<Vec<_>>();
        assert_eq!(codes.len(), 2, "{codes:#?}");
        assert_eq!(codes[0].1, "missing-file");
        assert!(codes[0].2.contains("m20261126_000003_ghost"));
        assert_eq!(codes[1].1, "unregistered");
        assert!(codes[1].2.contains("pub mod m20261126_000002_add_index;"));
        // 只登记在 mod 区块、没进列表的，提示指向列表。
        std::fs::write(
            src.join("lib.rs"),
            format!(
                "// <silent-boot:gen:mods>\n{MODS_ALL}// </silent-boot:gen:mods>\n\
                 // <silent-boot:gen:list>\nBox::new(m20261126_000001_create_article::Migration),\n// </silent-boot:gen:list>\n",
                MODS_ALL = "pub mod m20261126_000001_create_article;\npub mod m20261126_000002_add_index;\n"
            ),
        )
        .unwrap();
        let report = lint(&migration, None).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].advice.contains("迁移列表"));
    }

    #[test]
    fn missing_migration_crate_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let err = lint(&dir.path().join("migration"), None).unwrap_err();
        assert!(matches!(err, LintError::Io { .. }));
        assert!(err.to_string().contains("不是应用的迁移 crate"), "{err}");
    }
}
