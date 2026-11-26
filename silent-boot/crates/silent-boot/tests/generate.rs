//! M0 验证 V14：`generate entity` 的库级实现与基准命令的输出一致。
//!
//! 基准文件在 `tests/common/entity/`，由
//! `sea-orm-cli generate entity --with-serde both -v -o <目录> -l` 从同样的迁移生成的库里得到。
//!
//! `generate entity` 在 `codegen` feature 里，运行这个测试要带上它：
//! `cargo test -p silent-boot --features codegen --test generate`。
#![cfg(feature = "codegen")]

mod common;

use std::fs;
use std::path::Path;

use common::app::AppMigrator;
use silent_boot::cli::generate::{EntityOptions, GenerateError, generate_entity};
use silent_boot::db::migrate::{FrameworkMigrator, MigrationRunner};

const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/entity");
const FILES: [&str; 3] = ["article.rs", "lib.rs", "prelude.rs"];

fn names(files: &[std::path::PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|file| file.to_string_lossy().to_string())
        .collect()
}

#[tokio::test]
async fn v14_generate_entity_matches_the_baseline_command() {
    let (dir, db) = common::temp_db().await;
    MigrationRunner::new("v14")
        .up::<FrameworkMigrator, AppMigrator>(&db, None)
        .await
        .expect("migrate");
    let out = tempfile::tempdir().expect("temp dir");
    let target = out.path().join("entity");

    let generated = generate_entity(&common::url(&dir), &target, &EntityOptions::default())
        .await
        .expect("generate");
    assert_eq!(names(&generated.files), FILES);
    assert_eq!(
        names(&generated.changed),
        FILES,
        "a fresh directory means every file changes"
    );

    // 与基准命令的输出逐字节一致。
    for name in FILES {
        let mine = fs::read_to_string(target.join(name)).expect("generated file");
        let baseline = fs::read_to_string(Path::new(BASELINE).join(name)).expect("baseline file");
        assert_eq!(
            mine, baseline,
            "{name} differs from the baseline command's output"
        );
    }

    // 已经是最新时，再生成一次不会有任何变化。
    let again = generate_entity(&common::url(&dir), &target, &EntityOptions::default())
        .await
        .expect("generate again");
    assert!(again.changed.is_empty());

    // --dry-run 只报告，不写文件。
    let dry = tempfile::tempdir().expect("temp dir");
    let dry_run = generate_entity(
        &common::url(&dir),
        dry.path(),
        &EntityOptions {
            dry_run: true,
            ..Default::default()
        },
    )
    .await
    .expect("dry run");
    assert_eq!(names(&dry_run.changed), FILES);
    assert_eq!(
        fs::read_dir(dry.path()).expect("read_dir").count(),
        0,
        "dry run must not write files"
    );

    // 默认跳过的表即使写进 --tables 也不会生成。
    // 没有匹配到任何表时报错、不写文件：空着生成会把已有的 lib.rs 和 prelude.rs 清空。
    let picked = tempfile::tempdir().expect("temp dir");
    let err = generate_entity(
        &common::url(&dir),
        picked.path(),
        &EntityOptions {
            tables: vec!["sb_migration_log".to_owned()],
            ..Default::default()
        },
    )
    .await
    .expect_err("no tables should be an error");
    assert!(matches!(err, GenerateError::NoTables), "{err}");
    assert_eq!(fs::read_dir(picked.path()).expect("read_dir").count(), 0);

    // 只支持 SQLite。
    let err = generate_entity(
        "mysql://root@localhost/db",
        dry.path(),
        &EntityOptions::default(),
    )
    .await
    .expect_err("unsupported url");
    assert!(err.to_string().contains("sqlite"), "{err}");
}
