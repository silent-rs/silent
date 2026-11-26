//! M0 验证 V17：命令行子命令的入口。
//!
//! - 第一个参数不是框架的子命令时，`parse_from` 返回 `None`，应用照常启动服务。
//! - 帮助信息和用法错误如实上报，不会被当成"不是子命令"。
//! - 真实的二进制在命中子命令后执行并退出，不启动服务。
//!
//! 命令行在 `cli` feature 里，其中 `generate` 还需要 `codegen` feature。
#![cfg(feature = "cli")]

mod common;

use std::process::Command;

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
                dry_run: true
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
                dry_run: true
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

#[tokio::test]
async fn v17_binary_runs_subcommand_then_exits_without_a_server() {
    let bin = env!("CARGO_BIN_EXE_sb-sample");

    // 不带参数：走到"启动服务"的分支。
    let output = Command::new(bin).output().expect("run");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "no subcommand: the server would start here"
    );

    let (dir, _db) = common::temp_db().await;

    // 命中子命令但没配数据库：以 2 退出，不会启动服务。
    let output = Command::new(bin)
        .args(["migrate", "status"])
        .env_remove("SB_DATABASE_URL")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));

    // migrate up：建出框架表后退出。
    let output = Command::new(bin)
        .args(["migrate", "up"])
        .env("SB_DATABASE_URL", common::url(&dir))
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("framework: 1 applied"), "{stdout}");
    assert!(stdout.contains("app: 0 applied"), "{stdout}");

    // migrate status --json：输出结构化结果。
    let output = Command::new(bin)
        .args(["migrate", "status", "--json"])
        .env("SB_DATABASE_URL", common::url(&dir))
        .output()
        .expect("run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(json[0]["scope"], "framework");
    assert_eq!(json[0]["applied"].as_array().map(Vec::len), Some(1));
    assert_eq!(json[1]["scope"], "app");
}
