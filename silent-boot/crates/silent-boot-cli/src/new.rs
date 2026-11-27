//! `silent-boot new`：生成一个应用的骨架，见 `docs/design.md` §2.2。
//!
//! 骨架自带一个"文章"示例模块（迁移、实体、dto、service、api、接口测试），生成出来就能编译、能跑。
//! 模板在 [`crate::templates`]，只有占位符，没有逻辑。

use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

/// 生成出来的应用依赖的 SeaORM 版本，和 silent-boot 用的一致（有测试守着）。
pub const SEA_ORM_VERSION: &str = "2.0.4";

/// 应用开发依赖里 Silent 的版本范围，只用主次版本号（有测试守着）。
pub const SILENT_VERSION: &str = "2.16";
/// 应用的派生宏直接依赖的几个 crate 的版本，和 silent-boot 用的一致（有测试守着）。
pub const VALIDATOR_VERSION: &str = "0.21";
/// utoipa 的主版本号。
pub const UTOIPA_VERSION: &str = "6";

/// 用哪种数据库。
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Database {
    /// SQLite，默认。库是一个文件，开发和小工具够用。
    Sqlite,
    /// MySQL 8。
    Mysql,
    /// PostgreSQL。
    Postgres,
}

impl Database {
    fn framework_feature(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Mysql => "mysql",
            Self::Postgres => "postgres",
        }
    }

    fn sea_orm_feature(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlx-sqlite",
            Self::Mysql => "sqlx-mysql",
            Self::Postgres => "sqlx-postgres",
        }
    }

    /// 写进 `config/app.toml` 的默认地址。
    fn default_url(self, name: &str) -> String {
        match self {
            Self::Sqlite => "sqlite://data/app.db".to_owned(),
            Self::Mysql => format!("mysql://user:password@127.0.0.1:3306/{name}"),
            Self::Postgres => format!("postgres://user:password@127.0.0.1:5432/{name}"),
        }
    }
}

/// 应用怎么依赖 silent-boot 和 silent。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    /// 从 crates.io 取。
    Published,
    /// 用本地的目录，生成出来的应用用相对路径引用它们。
    Local {
        /// silent-boot 这个 crate 的目录（里面有 `Cargo.toml`）。
        silent_boot: PathBuf,
        /// silent 这个 crate 的目录。
        silent: PathBuf,
    },
}

impl DependencySource {
    /// `silent-boot = …` 等号右边的写法。
    fn silent_boot_spec(&self, database: Database, target: &Path) -> String {
        let features = format!(
            "[\"cli\", \"openapi\", \"compression\", \"{}\"]",
            database.framework_feature()
        );
        match self {
            Self::Published => format!(
                "{{ version = \"{}\", default-features = false, features = {features} }}",
                env!("CARGO_PKG_VERSION")
            ),
            Self::Local { silent_boot, .. } => format!(
                "{{ path = \"{}\", default-features = false, features = {features} }}",
                show_relative(silent_boot, target)
            ),
        }
    }

    /// 开发依赖里 `silent = …` 等号右边的写法，带 `test` feature（`TestClient` 在里面）。
    fn silent_spec(&self, target: &Path) -> String {
        match self {
            Self::Published => {
                format!("{{ version = \"{SILENT_VERSION}\", features = [\"test\"] }}")
            }
            Self::Local { silent, .. } => format!(
                "{{ path = \"{}\", features = [\"test\"] }}",
                show_relative(silent, target)
            ),
        }
    }
}

/// `dir` 相对 `base` 的路径，写成 `/` 分隔的形式；算不出来就用绝对路径。
///
/// 两边都先换成规范化的绝对路径：`base` 通常还没建出来，这时按当前目录拼出绝对路径。
fn show_relative(dir: &Path, base: &Path) -> String {
    fn absolute(path: &Path) -> PathBuf {
        if let Ok(real) = std::fs::canonicalize(path) {
            return real;
        }
        if path.is_absolute() {
            return path.to_path_buf();
        }
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
    let path = pathdiff::diff_paths(absolute(dir), absolute(base)).unwrap_or_else(|| absolute(dir));
    path.display().to_string().replace('\\', "/")
}

/// 生成应用的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 应用名，也是 Cargo 包名。
    pub name: String,
    /// 数据库。
    pub database: Database,
    /// 依赖从哪里来。
    pub deps: DependencySource,
    /// 应用要生成到哪个目录。用本地依赖时，相对路径按它来算。
    pub target: PathBuf,
}

/// 生成的应用名不合法，或者目标目录不为空。
#[derive(Debug, thiserror::Error)]
pub enum NewError {
    /// 应用名不能当 Cargo 包名。
    #[error("应用名 {0:?} 不合法：以字母开头，只能包含字母、数字、连字符和下划线")]
    InvalidName(String),
    /// 目标目录里已经有文件。
    #[error(
        "目录 {} 不是空的。加 --force 会覆盖脚手架生成的文件，其它文件不动",
        .0.display()
    )]
    DirectoryNotEmpty(PathBuf),
    /// 读写文件失败。
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// 一个要写出的文件。`path` 相对于应用根目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// 相对路径。
    pub path: PathBuf,
    /// 文件内容。
    pub content: String,
}

/// 算出一个应用的全部文件，不写盘。
pub fn scaffold(options: &Options) -> Result<Vec<GeneratedFile>, NewError> {
    if !is_valid_package_name(&options.name) {
        return Err(NewError::InvalidName(options.name.clone()));
    }
    let vars = [
        ("name", options.name.clone()),
        ("crate", options.name.replace('-', "_")),
        ("title", options.name.clone()),
        ("database_url", options.database.default_url(&options.name)),
        (
            "silent_boot",
            options
                .deps
                .silent_boot_spec(options.database, &options.target),
        ),
        ("silent_dev", options.deps.silent_spec(&options.target)),
        ("sea_orm", SEA_ORM_VERSION.to_owned()),
        ("sea_orm_db", options.database.sea_orm_feature().to_owned()),
        ("validator", VALIDATOR_VERSION.to_owned()),
        ("utoipa", UTOIPA_VERSION.to_owned()),
    ];
    Ok(crate::templates::app()
        .into_iter()
        .map(|template| GeneratedFile {
            path: PathBuf::from(template.path),
            content: render(template.body, &vars),
        })
        .collect())
}

/// 把模板里的 `{{ key }}` 换成对应的值。
pub(crate) fn render(template: &str, vars: &[(&str, String)]) -> String {
    vars.iter().fold(template.to_owned(), |out, (key, value)| {
        out.replace(&format!("{{{{ {key} }}}}"), value)
    })
}

fn is_valid_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// 把文件写到 `dir` 下。目录里有文件时拒绝，除非 `force`：那样只覆盖脚手架自己生成的文件。
/// 返回写出的路径。
pub fn write_to(
    dir: &Path,
    files: &[GeneratedFile],
    force: bool,
) -> Result<Vec<PathBuf>, NewError> {
    if !force && dir.is_dir() && has_entries(dir)? {
        return Err(NewError::DirectoryNotEmpty(dir.to_path_buf()));
    }
    let mut written = Vec::new();
    for file in files {
        let path = dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
        written.push(path);
    }
    Ok(written)
}

fn has_entries(dir: &Path) -> io::Result<bool> {
    Ok(std::fs::read_dir(dir)?.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(name: &str, database: Database, deps: DependencySource) -> Options {
        Options {
            name: name.to_owned(),
            database,
            deps,
            target: PathBuf::from("/nowhere/demo"),
        }
    }

    #[test]
    fn package_names_are_checked() {
        for good in ["demo", "my_app", "admin-2", "_private"] {
            assert!(is_valid_package_name(good), "{good}");
        }
        for bad in ["", "2demo", "my app", "demo/app", "应用"] {
            assert!(!is_valid_package_name(bad), "{bad}");
        }
        let err = scaffold(&options(
            "2demo",
            Database::Sqlite,
            DependencySource::Published,
        ))
        .expect_err("the name is rejected");
        assert!(matches!(err, NewError::InvalidName(_)), "{err}");
    }

    #[test]
    fn every_placeholder_is_replaced() {
        for database in [Database::Sqlite, Database::Mysql, Database::Postgres] {
            let files = scaffold(&options("demo", database, DependencySource::Published)).unwrap();
            let read = |path: &str| {
                files
                    .iter()
                    .find(|f| f.path == Path::new(path))
                    .unwrap_or_else(|| panic!("{path} is generated"))
                    .content
                    .clone()
            };
            for file in &files {
                assert!(
                    !file.content.contains("{{ "),
                    "{} still has a placeholder:\n{}",
                    file.path.display(),
                    file.content
                );
                assert!(!file.content.contains('\r'), "{}", file.path.display());
            }
            let manifest = read("Cargo.toml");
            assert!(manifest.contains("name = \"demo\""), "{manifest}");
            assert!(
                manifest.contains(&format!("\"{}\"", database.sea_orm_feature())),
                "{manifest}"
            );
            assert!(
                manifest.contains(&format!("\"{}\"", database.framework_feature())),
                "{manifest}"
            );
            assert!(read("config/app.toml").contains(&database.default_url("demo")));
            assert!(read("src/app.rs").contains("title = \"demo\""));
            assert!(read("silent-boot.toml").contains("name = \"demo\""));
            assert_eq!(files.len(), crate::templates::app().len());
        }
        // 模板里不留具体的取值：把生成结果拷回模板时，漏掉换回占位符的会在这里暴露。
        for template in crate::templates::app() {
            assert!(
                !template.body.contains("demo"),
                "{} mentions a concrete app name",
                template.path
            );
        }
    }

    #[test]
    fn local_dependencies_become_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let target = repo.join("silent-boot/examples/demo");
        std::fs::create_dir_all(&target).unwrap();
        let deps = DependencySource::Local {
            silent_boot: repo.join("silent-boot/crates/silent-boot"),
            silent: repo.join("silent"),
        };
        // 目录还不存在时算不出规范路径，退回到按字面算相对路径。
        assert_eq!(
            deps.silent_boot_spec(Database::Mysql, &target),
            "{ path = \"../../crates/silent-boot\", default-features = false, features = [\"cli\", \"openapi\", \"compression\", \"mysql\"] }"
        );
        assert_eq!(
            deps.silent_spec(&target),
            "{ path = \"../../../silent\", features = [\"test\"] }"
        );
        let files = scaffold(&Options {
            name: "demo".to_owned(),
            database: Database::Postgres,
            deps,
            target: target.clone(),
        })
        .unwrap();
        let manifest = files
            .iter()
            .find(|f| f.path == Path::new("Cargo.toml"))
            .unwrap();
        assert!(
            manifest
                .content
                .contains("path = \"../../crates/silent-boot\"")
        );
        assert!(manifest.content.contains("\"sqlx-postgres\""));
        // 开发依赖那一行要有 crate 名。
        assert!(
            manifest
                .content
                .contains("silent = { path = \"../../../silent\", features = [\"test\"] }"),
            "{}",
            manifest.content
        );
    }

    #[test]
    fn write_refuses_a_busy_directory_unless_forced() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo");
        let files = scaffold(&options(
            "demo",
            Database::Sqlite,
            DependencySource::Published,
        ))
        .unwrap();
        let written = write_to(&dir, &files, false).unwrap();
        assert_eq!(written.len(), files.len());
        assert!(matches!(
            write_to(&dir, &files, false),
            Err(NewError::DirectoryNotEmpty(_))
        ));
        // --force 只覆盖脚手架自己的文件。
        std::fs::write(dir.join("notes.md"), "keep me").unwrap();
        std::fs::write(dir.join("src/main.rs"), "edited").unwrap();
        write_to(&dir, &files, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("notes.md")).unwrap(),
            "keep me"
        );
        assert!(
            std::fs::read_to_string(dir.join("src/main.rs"))
                .unwrap()
                .contains("silent_boot::run")
        );
    }

    #[test]
    fn versions_match_the_framework() {
        // 守着 SEA_ORM_VERSION 和 SILENT_VERSION：silent-boot 升级依赖时这里会先红。
        let manifest = include_str!("../../../Cargo.toml");
        let line = manifest
            .lines()
            .find(|l| l.starts_with("sea-orm = { version = "))
            .unwrap();
        assert!(
            line.contains(&format!("version = \"{SEA_ORM_VERSION}\"")),
            "{line}"
        );
        // Silent 的版本写在外层 workspace 的 `[workspace.package]` 里。
        let outer = include_str!("../../../../Cargo.toml");
        let workspace_package = outer
            .split("[workspace.package]")
            .nth(1)
            .and_then(|rest| rest.split("\n[").next())
            .unwrap_or_default();
        let version = workspace_package
            .lines()
            .find(|l| l.starts_with("version = \""))
            .map(|l| l.trim_start_matches("version = \"").trim_end_matches('"'))
            .expect("the outer workspace declares its version");
        let mut parts = version.split('.');
        let major = parts.next().unwrap();
        let minor = parts.next().unwrap();
        assert_eq!(SILENT_VERSION, format!("{major}.{minor}"));
        // 发布出来的版本就是工具自己的版本：工具和框架在同一个 workspace 里，共用 `[workspace.package]` 的版本。
        let workspace_version = manifest
            .split("[workspace.package]")
            .nth(1)
            .and_then(|rest| rest.split("\n[").next())
            .and_then(|section| {
                section
                    .lines()
                    .find(|l| l.starts_with("version = \""))
                    .map(|l| l.trim_start_matches("version = \"").trim_end_matches('"'))
            })
            .expect("the workspace declares its version");
        assert_eq!(workspace_version, env!("CARGO_PKG_VERSION"));
        assert!(
            include_str!("../../silent-boot/Cargo.toml").contains("version.workspace = true"),
            "silent-boot takes its version from the workspace"
        );
    }

    #[test]
    fn entity_templates_match_the_generator_baseline() {
        // 模板里的实体就是 `generate entity` 对示例表的输出；基准文件变了要同步。
        assert_eq!(
            include_str!("../templates/app/entity/src/article.rs"),
            include_str!("../../silent-boot/tests/common/entity/article.rs")
        );
        assert_eq!(
            include_str!("../templates/app/entity/src/lib.rs"),
            include_str!("../../silent-boot/tests/common/entity/lib.rs")
        );
        assert_eq!(
            include_str!("../templates/app/entity/src/prelude.rs"),
            include_str!("../../silent-boot/tests/common/entity/prelude.rs")
        );
    }
}
