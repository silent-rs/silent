//! M1 验证：应用内子命令的执行（不经过进程），以及真实二进制的入口。
//!
//! - `migrate up`、`migrate status` 的输出；
//! - `migrate baseline` 只改记录、不建表；
//! - `migrate repair` 把一条迁移标记成已执行或未执行；
//! - `migrate lint` 找出没标记的破坏性操作，`--since` 只看新增的文件；
//! - `healthcheck` 探测一个正在运行的应用；
//! - 数据库地址的优先级：`--database-url`、`SB_DATABASE_URL`、配置文件；
//! - 真实的二进制：`--help` 列出子命令；不带你参数时启动服务，能被 `healthcheck` 探到。
#![cfg(feature = "cli")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use common::app::AppMigrator;
use sea_orm::{ConnectionTrait, Database, Statement};
use silent_boot::cli::{self, CommandContext};
use silent_boot::config::{AppConfig, AutoApply, DatabaseConfig, MigrateConfig};
use silent_boot::db::migrate::AnyMigrator;
use silent_boot::prelude::*;
use tokio::io::{AsyncBufReadExt, BufReader};

fn context(root: &Path, url: &str) -> CommandContext {
    CommandContext::with_env(
        root,
        AnyMigrator::new(AppMigrator),
        [(cli::DATABASE_URL_ENV.to_owned(), url.to_owned())],
    )
}

/// 执行一条子命令，返回结果和它写到输出里的文字。
async fn run(
    args: &[&str],
    ctx: &CommandContext,
) -> (std::result::Result<(), cli::CommandError>, String) {
    let cli = cli::parse_from(std::iter::once("app").chain(args.iter().copied()))
        .expect("valid arguments")
        .expect("a framework subcommand");
    let mut out = Vec::new();
    let result = cli::dispatch(cli, ctx, &mut out).await;
    (result, String::from_utf8(out).expect("output is utf-8"))
}

/// 在 `url` 上执行一条原生 SQL，返回所有行。
async fn raw(url: &str, sql: impl Into<String>) -> Vec<sea_orm::QueryResult> {
    let db = Database::connect(url).await.expect("connect");
    db.query_all_raw(Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        sql.into(),
    ))
    .await
    .expect("query")
}

async fn table_names(url: &str) -> Vec<String> {
    raw(
        url,
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .await
    .iter()
    .map(|row| row.try_get::<String>("", "name").expect("name"))
    .collect()
}

async fn tracked(url: &str, table: &str) -> Vec<String> {
    raw(url, format!("SELECT version FROM {table} ORDER BY version"))
        .await
        .iter()
        .map(|row| row.try_get::<String>("", "version").expect("version"))
        .collect()
}

const SEED: &str = "m20261125_120001_seed_article";
const FRAMEWORK_LOG: &str = "m20261125_000001_create_sb_migration_log";

#[tokio::test]
async fn v28_up_and_status_report_both_groups() {
    let (dir, _db) = common::temp_db().await;
    let ctx = context(dir.path(), &common::url(&dir));
    let (result, out) = run(&["migrate", "up"], &ctx).await;
    result.expect("up");
    assert!(out.contains("framework: 1 applied"), "{out}");
    assert!(out.contains(FRAMEWORK_LOG), "{out}");
    assert!(out.contains("app: 2 applied"), "{out}");
    assert!(out.contains(SEED), "{out}");

    let (result, out) = run(&["migrate", "status"], &ctx).await;
    result.expect("status");
    assert!(out.contains(&format!("applied  {SEED}")), "{out}");
    assert!(!out.contains("pending  "), "{out}");

    let (result, out) = run(&["migrate", "status", "--json"], &ctx).await;
    result.expect("status json");
    let json: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(json[0]["scope"], "framework");
    assert_eq!(json[0]["applied"].as_array().map(Vec::len), Some(1));
    assert_eq!(json[1]["scope"], "app");
    assert_eq!(json[1]["pending"], serde_json::json!([]));

    // `--steps` 对两组分别生效。
    let (dir, _db) = common::temp_db().await;
    let ctx = context(dir.path(), &common::url(&dir));
    let (result, out) = run(&["migrate", "up", "--steps", "1"], &ctx).await;
    result.expect("up one step");
    assert!(out.contains("framework: 1 applied"), "{out}");
    assert!(out.contains("app: 1 applied"), "{out}");
    let (result, out) = run(&["migrate", "up", "--dry-run"], &ctx).await;
    result.expect("dry run");
    assert!(out.contains("framework"), "{out}");
    assert!(out.contains("up to date"), "{out}");
    assert!(out.contains("pending  "), "{out}");
}

#[tokio::test]
async fn v29_baseline_marks_pending_migrations_without_running_them() {
    let (dir, _db) = common::temp_db().await;
    let url = common::url(&dir);
    let ctx = context(dir.path(), &url);
    let (result, out) = run(&["migrate", "baseline"], &ctx).await;
    result.expect("baseline");
    assert!(
        out.contains("baseline: 2 migration(s) marked as applied"),
        "{out}"
    );
    assert!(out.contains("seaql_migrations"), "{out}");
    assert!(out.contains(SEED), "{out}");
    assert!(out.contains("migrate up"), "{out}");

    // 只写了记录，表没有建。
    let names = table_names(&url).await;
    assert!(names.contains(&"seaql_migrations".to_owned()), "{names:?}");
    assert!(!names.contains(&"article".to_owned()), "{names:?}");
    assert!(!names.contains(&"sb_migrations".to_owned()), "{names:?}");
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 2);

    // 之后再 `migrate up`：只执行框架迁移，应用的那两条不会再跑。
    let (result, out) = run(&["migrate", "up"], &ctx).await;
    result.expect("up");
    assert!(out.contains("framework: 1 applied"), "{out}");
    assert!(out.contains("app: 0 applied"), "{out}");

    // 没有待执行的迁移时什么都不做。
    let (result, out) = run(&["migrate", "baseline"], &ctx).await;
    result.expect("baseline again");
    assert!(out.contains("0 migration(s)"), "{out}");
    assert!(out.contains("nothing to do"), "{out}");
}

#[tokio::test]
async fn v30_repair_marks_one_migration_applied_or_pending() {
    let (dir, _db) = common::temp_db().await;
    let url = common::url(&dir);
    let ctx = context(dir.path(), &url);
    run(&["migrate", "up"], &ctx).await.0.expect("up");

    let (result, out) = run(&["migrate", "repair", "--pending", SEED], &ctx).await;
    result.expect("repair");
    assert!(
        out.contains(&format!("{SEED} is now marked as pending")),
        "{out}"
    );
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 1);
    let (_, out) = run(&["migrate", "status"], &ctx).await;
    assert!(out.contains(&format!("pending  {SEED}")), "{out}");

    // 撤掉记录之后 `migrate up` 会重跑这一条。
    let (result, out) = run(&["migrate", "up"], &ctx).await;
    result.expect("up again");
    assert!(out.contains("app: 1 applied"), "{out}");
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 2);

    let (result, out) = run(&["migrate", "repair", "--applied", SEED], &ctx).await;
    result.expect("repair");
    assert!(
        out.contains(&format!("{SEED} was already applied")),
        "{out}"
    );

    // 框架那一组要显式指定。
    let args = [
        "migrate",
        "repair",
        "--pending",
        FRAMEWORK_LOG,
        "--scope",
        "framework",
    ];
    let (result, out) = run(&args, &ctx).await;
    result.expect("repair framework");
    assert!(
        out.contains("is now marked as pending (Framework)"),
        "{out}"
    );
    assert!(tracked(&url, "sb_migrations").await.is_empty());
    let (_, out) = run(&["migrate", "status"], &ctx).await;
    assert!(out.contains(&format!("pending  {FRAMEWORK_LOG}")), "{out}");

    // 两个都没给：说明怎么用。
    let (result, _) = run(&["migrate", "repair"], &ctx).await;
    let err = result.expect_err("repair needs a version");
    assert!(err.to_string().contains("--applied"), "{err}");
}

fn write_migration_workspace(root: &Path) -> PathBuf {
    let src = root.join("migration/src");
    std::fs::create_dir_all(&src).expect("migration/src");
    std::fs::write(
        src.join("lib.rs"),
        "mod helper;\n\
         // <silent-boot:gen:mods>\n\
         pub mod m20261126_000001_create_article;\n\
         pub mod m20261126_000002_drop_note;\n\
         // </silent-boot:gen:mods>\n\
         // <silent-boot:gen:list>\n\
         Box::new(m20261126_000001_create_article::Migration),\n\
         Box::new(m20261126_000002_drop_note::Migration),\n\
         // </silent-boot:gen:list>\n",
    )
    .expect("lib.rs");
    std::fs::write(
        src.join("m20261126_000001_create_article.rs"),
        "Table::create().table(A::Article).to_owned()\n",
    )
    .expect("create");
    std::fs::write(
        src.join("m20261126_000002_drop_note.rs"),
        "Table::drop().table(A::Note).to_owned()\n",
    )
    .expect("drop");
    root.join("migration")
}

#[tokio::test]
async fn v31_lint_finds_unmarked_destructive_migrations() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_migration_workspace(dir.path());
    let ctx = context(dir.path(), "sqlite::memory:");
    let (result, out) = run(&["migrate", "lint"], &ctx).await;
    let err = result.expect_err("lint must fail");
    assert!(err.to_string().contains("1 个问题"), "{err}");
    assert!(
        out.contains("m20261126_000002_drop_note.rs:1: drop-table: 删表"),
        "{out}"
    );
    assert!(out.contains("// silent-boot: destructive"), "{out}");
    assert!(out.contains("检查了 2 个迁移文件，发现 1 个问题"), "{out}");

    // 加上标记就通过。
    std::fs::write(
        dir.path()
            .join("migration/src/m20261126_000002_drop_note.rs"),
        "// silent-boot: destructive\nTable::drop().table(A::Note).to_owned()\n",
    )
    .expect("drop");
    let (result, out) = run(&["migrate", "lint"], &ctx).await;
    result.expect("lint passes");
    assert!(out.contains("没有问题"), "{out}");

    // 没有迁移 crate 时说清楚。
    let empty = tempfile::tempdir().expect("temp dir");
    let ctx = context(empty.path(), "sqlite::memory:");
    let (result, _) = run(&["migrate", "lint"], &ctx).await;
    let err = result.expect_err("no migration crate");
    assert!(err.to_string().contains("不是应用的迁移 crate"), "{err}");
}

#[tokio::test]
async fn v31_lint_since_only_checks_files_added_after_a_git_ref() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_migration_workspace(dir.path());
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .expect("git");
        assert!(output.status.success(), "git {args:?}: {:?}", output.status);
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "base"]);
    std::fs::write(
        dir.path()
            .join("migration/src/m20261126_000003_truncate_log.rs"),
        "Table::truncate().table(A::Log).to_owned()\n",
    )
    .expect("truncate");
    let ctx = context(dir.path(), "sqlite::memory:");

    // 全量检查有两条；相对 HEAD 只有新增的那一条。
    let (result, out) = run(&["migrate", "lint"], &ctx).await;
    // 全量：两条破坏性操作，外加新文件没登记。
    assert!(result.is_err());
    assert!(out.contains("发现 3 个问题"), "{out}");
    assert!(out.contains("unregistered"), "{out}");
    let (result, out) = run(&["migrate", "lint", "--since", "HEAD"], &ctx).await;
    assert!(result.is_err());
    assert!(out.contains("检查了 1 个迁移文件，发现 1 个问题"), "{out}");
    assert!(
        out.contains("m20261126_000003_truncate_log.rs:1: truncate"),
        "{out}"
    );
    assert!(!out.contains("m20261126_000002_drop_note"), "{out}");

    // 新文件带上标记之后，只看新增文件的检查通过：没登记也不管，那是全量检查的事。
    std::fs::write(
        dir.path()
            .join("migration/src/m20261126_000003_truncate_log.rs"),
        "// silent-boot: destructive\nTable::truncate().table(A::Log).to_owned()\n",
    )
    .expect("truncate");
    let (result, out) = run(&["migrate", "lint", "--since", "HEAD"], &ctx).await;
    result.expect("only the new file is checked, and it is marked");
    assert!(out.contains("没有问题"), "{out}");

    // 引用不存在时把 git 的原话带出来。
    let (result, _) = run(&["migrate", "lint", "--since", "no-such-ref"], &ctx).await;
    let err = result.expect_err("bad ref");
    assert!(err.to_string().contains("no-such-ref"), "{err}");
}

/// 起一个真实的应用，返回它的地址和停止它的句柄。
async fn start_app(dir: &Path, addr: std::net::SocketAddr) -> (std::net::SocketAddr, impl Fn()) {
    let config = AppConfig {
        database: DatabaseConfig {
            url: format!("sqlite://{}/data/app.db", dir.display()),
            migrate: MigrateConfig {
                auto_apply: AutoApply::Always,
                ..MigrateConfig::default()
            },
            ..DatabaseConfig::default()
        },
        server: silent_boot::config::ServerConfig {
            listen: addr.to_string(),
            ..Default::default()
        },
        ..AppConfig::default()
    };
    let app = App::with_config(config)
        .migrator(AppMigrator)
        .build()
        .await
        .expect("build");
    let listener = app.bind().await.expect("bind");
    let local = listener.local_addr().expect("local addr");
    let stopped = std::sync::Arc::new(tokio::sync::Notify::new());
    let waiter = stopped.clone();
    tokio::spawn(app.serve(listener, async move {
        waiter.notified().await;
    }));
    (local, move || stopped.notify_one())
}

async fn free_port() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    listener.local_addr().expect("addr")
}

#[tokio::test]
async fn v32_healthcheck_probes_a_running_app_and_reads_listen_from_config() {
    let dir = tempfile::tempdir().expect("temp dir");
    let port = free_port().await;
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("app.toml"),
        format!(
            "[app]\nname = \"demo\"\n\n[server]\nlisten = \"{}\"\n",
            port
        ),
    )
    .expect("app.toml");
    let ctx = context(dir.path(), "sqlite::memory:");
    let (addr, stop) = start_app(dir.path(), port).await;
    assert_eq!(addr, port);

    let (result, out) = run(&["healthcheck"], &ctx).await;
    result.expect("healthy");
    assert!(
        out.contains(&format!("healthy: {addr}/healthz returned 200")),
        "{out}"
    );
    let (result, out) = run(
        &[
            "healthcheck",
            "--url",
            &format!("http://{addr}/readyz"),
            "--timeout",
            "5s",
        ],
        &ctx,
    )
    .await;
    result.expect("ready");
    assert!(out.contains("returned 200"), "{out}");

    // 服务停了以后探测失败，错误里带着原因。
    stop();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (result, _) = run(
        &[
            "healthcheck",
            "--url",
            &format!("http://{addr}/healthz"),
            "--timeout",
            "1s",
        ],
        &ctx,
    )
    .await;
    let err = result.expect_err("unhealthy");
    assert!(err.to_string().contains("连不上"), "{err}");

    let (result, _) = run(&["healthcheck", "--url", "https://example.com"], &ctx).await;
    assert!(result.expect_err("https").to_string().contains("只支持"));
    let (result, _) = run(&["healthcheck", "--timeout", "soon"], &ctx).await;
    assert!(
        result
            .expect_err("bad duration")
            .to_string()
            .contains("30s")
    );
}

#[tokio::test]
async fn v33_database_url_precedence_and_config_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    let from_config = format!("sqlite://{}/config.db?mode=rwc", dir.path().display());
    let from_env = format!("sqlite://{}/env.db?mode=rwc", dir.path().display());
    let from_flag = format!("sqlite://{}/flag.db?mode=rwc", dir.path().display());
    std::fs::write(
        config_dir.join("app.toml"),
        format!("[database]\nurl = '{from_config}'\n"),
    )
    .expect("app.toml");

    // 只有配置文件。
    let only_config = CommandContext::with_env(
        dir.path(),
        AnyMigrator::new(AppMigrator),
        Vec::<(String, String)>::new(),
    );
    run(&["migrate", "up"], &only_config).await.0.expect("up");
    assert!(
        table_names(&from_config)
            .await
            .contains(&"article".to_owned())
    );

    // 环境变量压过配置文件。
    let with_env = context(dir.path(), &from_env);
    run(&["migrate", "up"], &with_env).await.0.expect("up");
    assert!(table_names(&from_env).await.contains(&"article".to_owned()));

    // 命令行压过环境变量。
    let (result, out) = run(&["migrate", "up", "--database-url", &from_flag], &with_env).await;
    result.expect("up with flag");
    assert!(out.contains("framework: 1 applied"), "{out}");
    assert!(
        table_names(&from_flag)
            .await
            .contains(&"article".to_owned())
    );

    // 配置里的地址和编译时启用的 feature 对不上：说清楚，不去连库。
    let (result, _) = run(
        &["migrate", "status", "--database-url", "mysql://u:p@h/db"],
        &with_env,
    )
    .await;
    if cfg!(feature = "mysql") {
        // 带上 mysql feature 时这个地址是合法的，会真的去连库（这里连不上，但原因不再是 feature）。
        let err = result.expect_err("no mysql server is running");
        assert!(!err.to_string().contains("feature"), "{err}");
    } else {
        let err = result.expect_err("mysql without the feature");
        assert!(err.to_string().contains("feature"), "{err}");
    }
}

#[tokio::test]
async fn v34_binary_lists_subcommands_and_starts_a_server_that_healthcheck_can_see() {
    let bin = env!("CARGO_BIN_EXE_sb-sample");
    let dir = tempfile::tempdir().expect("temp dir");

    let output = std::process::Command::new(bin)
        .args(["--help"])
        .current_dir(dir.path())
        .output()
        .expect("help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for word in ["migrate", "healthcheck"] {
        assert!(help.contains(word), "{word} is listed:\n{help}");
    }
    let output = std::process::Command::new(bin)
        .args(["migrate", "--help"])
        .current_dir(dir.path())
        .output()
        .expect("migrate help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for word in ["generate", "up", "status", "lint", "baseline", "repair"] {
        assert!(help.contains(word), "{word} is listed:\n{help}");
    }

    // 用不了的地址：报错并以 1 退出。
    let output = std::process::Command::new(bin)
        .args(["migrate", "status", "--database-url", "mysql://u@h/db"])
        .current_dir(dir.path())
        .output()
        .expect("status");
    if !cfg!(feature = "mysql") {
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("mysql"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 不带参数：读 config/app.toml 启动服务，日志里报出监听地址；用子命令探到它，再停掉。
    let port = free_port().await;
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("app.toml"),
        format!(
            "[app]\nname = \"sample\"\n\n[server]\nlisten = \"{port}\"\n\n[database]\nurl = 'sqlite://{}/data/app.db'\n",
            dir.path().display()
        ),
    )
    .expect("app.toml");
    let mut child = tokio::process::Command::new(bin)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = BufReader::new(stdout).lines();
    let mut seen = Vec::new();
    let listening = loop {
        let line = tokio::time::timeout(Duration::from_secs(120), lines.next_line())
            .await
            .expect("the app logs its listen address")
            .expect("read stdout")
            .expect("the app keeps running");
        seen.push(line.clone());
        if line.contains("listening on") {
            break line;
        }
    };
    assert!(
        listening.contains(&format!("http://{port}")),
        "{listening}\n{}",
        seen.join("\n")
    );
    assert!(
        dir.path().join("data/app.db").exists(),
        "the database file was created"
    );

    // 配置文件里就是监听地址，所以不用再传 `--url`。
    let ctx = CommandContext::with_env(
        dir.path(),
        AnyMigrator::new(AppMigrator),
        Vec::<(String, String)>::new(),
    );
    let (result, out) = run(&["healthcheck"], &ctx).await;
    result.expect("the started app answers /healthz");
    assert!(out.contains("returned 200"), "{out}");

    let _ = child.kill().await;
    assert!(
        !seen.iter().any(|l| l.contains("ERROR")),
        "no errors in the startup log:\n{}",
        seen.join("\n")
    );
}

async fn count(url: &str, sql: &str) -> i64 {
    raw(url, sql).await[0]
        .try_get::<i64>("", "c")
        .expect("count")
}

/// 两个真实的进程同时执行 `migrate up`：一个执行，另一个等到锁之后发现没有事可做。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v35_two_processes_migrate_at_once_and_only_one_applies() {
    let dir = tempfile::tempdir().expect("temp dir");
    let url = format!("sqlite://{}/data/app.db", dir.path().display());
    let bin = env!("CARGO_BIN_EXE_sb-sample");
    let spawn = || {
        std::process::Command::new(bin)
            .args(["migrate", "up", "--database-url", &url])
            .current_dir(dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn")
    };
    let first = spawn();
    let second = spawn();
    let outputs = [
        first.wait_with_output().expect("first"),
        second.wait_with_output().expect("second"),
    ];
    let applied = |output: &std::process::Output| {
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(|line| {
                line.strip_prefix("framework: ")?
                    .strip_suffix(" applied")?
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or_else(|| panic!("no framework line in {text}"))
    };
    let mut counts = [applied(&outputs[0]), applied(&outputs[1])];
    counts.sort();
    assert_eq!(counts, [0, 1], "exactly one process applies the migration");
    for output in &outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(tracked(&url, "sb_migrations").await.len(), 1);
    // 每个执行器给两组迁移各留一行执行记录：什么都没执行的那次也留。
    assert_eq!(
        count(&url, "SELECT COUNT(*) AS c FROM sb_migration_log").await,
        4
    );
    // 锁已经释放：表里没有还占着的行。
    assert_eq!(count(&url, "SELECT COUNT(*) AS c FROM sb_locks").await, 0);
}

#[tokio::test]
async fn v37_migrate_down_reverts_in_development_and_is_refused_elsewhere() {
    use common::reversible::ReversibleMigrator;

    let dir = tempfile::tempdir().expect("temp dir");
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let vars = [(cli::DATABASE_URL_ENV.to_owned(), url.clone())];
    let dev = CommandContext::with_env(
        dir.path(),
        AnyMigrator::new(ReversibleMigrator),
        vars.clone(),
    );
    let (result, _) = run(&["migrate", "up"], &dev).await;
    result.expect("migrate up");
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 2);

    // 默认回退一条：最后执行的那条。
    let (result, out) = run(&["migrate", "down"], &dev).await;
    result.expect("migrate down");
    assert!(out.contains("app: 1 reverted"), "{out}");
    assert!(out.contains("m20261127_000002_create_note"), "{out}");
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 1);

    // 不是开发环境：拒绝，连库之前就拒绝，记录不变。
    let production = vars
        .clone()
        .into_iter()
        .chain([("APP_ENV".to_owned(), "production".to_owned())]);
    let prod =
        CommandContext::with_env(dir.path(), AnyMigrator::new(ReversibleMigrator), production);
    let (result, _) = run(&["migrate", "down", "--steps", "5"], &prod).await;
    let err = result.expect_err("down is refused outside development");
    assert!(err.to_string().contains("只能在开发环境"), "{err}");
    assert!(err.to_string().contains("production"), "{err}");
    assert_eq!(tracked(&url, "seaql_migrations").await.len(), 1);

    // `--steps` 至少是 1。
    assert!(
        cli::parse_from(["app", "migrate", "down", "--steps", "0"]).is_err(),
        "--steps 0 must be rejected"
    );
}
