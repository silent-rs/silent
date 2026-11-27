//! M0 验证 V17：命令行子命令的入口。
//!
//! - 第一个参数不是框架的子命令时，`parse_from` 返回 `None`，应用照常启动服务。
//! - 帮助信息和用法错误如实上报，不会被当成"不是子命令"。
//!
//! 子命令的执行和真实二进制的启动在 `tests/cli_dispatch.rs` 里验证。
//!
//! 命令行在 `cli` feature 里，其中 `generate` 还需要 `codegen` feature。
#![cfg(feature = "cli")]

mod common;

use std::fs;

use chrono::NaiveDateTime;
use clap::error::ErrorKind;
#[cfg(feature = "codegen")]
use silent_boot::cli::Generate;
use silent_boot::cli::{Command as CliCommand, Migrate, parse_from};

#[test]
fn v17_unknown_first_argument_means_start_the_server() {
    assert!(parse_from(["app"]).unwrap().is_none());
    assert!(
        parse_from(["app", "serve", "--port", "8080"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn v17_known_subcommands_are_parsed() {
    let cli = parse_from(["app", "migrate", "up", "--steps", "1", "--dry-run"])
        .unwrap()
        .expect("cli");
    assert!(matches!(
        cli.command,
        CliCommand::Migrate {
            action: Migrate::Up {
                steps: Some(1),
                dry_run: true,
                ..
            }
        }
    ));
}

#[test]
#[cfg(feature = "codegen")]
fn v17_generate_subcommand_is_parsed_with_the_codegen_feature() {
    let cli = parse_from([
        "app",
        "generate",
        "entity",
        "--tables",
        "a,b",
        "--ignore-tables",
        "c",
        "--dry-run",
    ])
    .unwrap()
    .expect("cli");
    assert!(matches!(
        cli.command,
        CliCommand::Generate {
            action: Generate::Entity {
                ref tables,
                ref ignore_tables,
                dry_run: true,
                ..
            }
        } if tables == &["a".to_owned(), "b".to_owned()]
            && ignore_tables == &["c".to_owned()]
    ));
}

#[test]
fn v17_help_and_usage_errors_are_reported() {
    // 缺少子命令时 clap 给出的是"显示帮助"，parse() 会把它打印到标准错误并以 2 退出。
    let err = parse_from(["app", "migrate"]).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    let err = parse_from(["app", "migrate", "up", "--nope"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    let err = parse_from(["app", "--help"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayHelp);
    let err = parse_from(["app", "--version"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
}

/// 脚手架生成的迁移器 `src/lib.rs`：两个受管区块由 `migrate generate` 维护，其余部分归应用。
const MIGRATOR_TEMPLATE: &str = "\
//! 应用的迁移器。
use silent_boot::db::migrate::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            // <silent-boot:gen:list>
            // </silent-boot:gen:list>
        ]
    }
}

// <silent-boot:gen:mods>
// </silent-boot:gen:mods>
";

#[test]
fn v5_migrate_generate_writes_the_file_and_registers_it() {
    use silent_boot::cli::migrate::{self, GenerateError};

    let crate_dir = tempfile::tempdir().expect("temp dir");
    let src = crate_dir.path().join("src");
    fs::create_dir_all(&src).expect("src dir");
    fs::write(src.join("lib.rs"), MIGRATOR_TEMPLATE).expect("lib.rs");
    let at =
        NaiveDateTime::parse_from_str("2026-11-26 09:30:00", "%Y-%m-%d %H:%M:%S").expect("time");

    let first = migrate::generate("create article", at, crate_dir.path()).expect("generate");
    assert_eq!(first.module, "m20261126_093000_create_article");
    // 骨架与参与编译的那份逐字相同，只差说明文字。
    let body = fs::read_to_string(&first.file).expect("migration file");
    let compiled = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/common/skeleton/m20261125_120000_skeleton.rs"
    ))
    .expect("skeleton fixture");
    assert_eq!(body.replace("/// create article", "/// skeleton"), compiled);

    // 时间更早的迁移排在前面，先执行。
    let second = migrate::generate(
        "add extra column",
        at - chrono::Duration::seconds(30),
        crate_dir.path(),
    )
    .expect("generate");
    assert_eq!(second.module, "m20261126_092930_add_extra_column");
    let lib = fs::read_to_string(src.join("lib.rs")).expect("lib.rs");
    let position = |needle: &str| {
        lib.find(needle)
            .unwrap_or_else(|| panic!("{needle} is not registered"))
    };
    assert!(
        position("pub mod m20261126_092930_add_extra_column;")
            < position("pub mod m20261126_093000_create_article;")
    );
    assert!(
        position("Box::new(m20261126_092930_add_extra_column::Migration),")
            < position("Box::new(m20261126_093000_create_article::Migration),")
    );
    // 区块之外的内容原样保留。
    assert!(lib.starts_with("//! 应用的迁移器。"));
    assert!(lib.contains("pub struct Migrator;"));
    assert!(lib.ends_with("// </silent-boot:gen:mods>\n"));

    // 名称不合法；同一秒已有同名文件。
    assert!(matches!(
        migrate::generate("bad-name", at, crate_dir.path()),
        Err(GenerateError::InvalidName(_))
    ));
    assert!(matches!(
        migrate::generate("create article", at, crate_dir.path()),
        Err(GenerateError::AlreadyExists(_))
    ));

    // 迁移器里没有受管区块：报错，而且不留下任何迁移文件。
    let bare = tempfile::tempdir().expect("temp dir");
    fs::create_dir_all(bare.path().join("src")).expect("src dir");
    fs::write(bare.path().join("src/lib.rs"), "pub struct Migrator;\n").expect("lib.rs");
    assert!(matches!(
        migrate::generate("x", at, bare.path()),
        Err(GenerateError::MissingMarkers(_))
    ));
    assert_eq!(
        fs::read_dir(bare.path().join("src"))
            .expect("read_dir")
            .count(),
        1,
        "only lib.rs should exist"
    );
}
