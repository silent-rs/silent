//! 集成测试共用的夹具：临时 SQLite 库和样例应用的迁移。
#![allow(dead_code)]

pub mod app;
#[path = "entity/lib.rs"]
#[allow(unused_imports)]
pub mod entity;
pub mod fail;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// 建一个 SQLite 临时文件库。返回的 `TempDir` 被丢弃时文件随之删除。
pub async fn temp_db() -> (tempfile::TempDir, DatabaseConnection) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let db = connect(&url).await;
    (dir, db)
}

/// 再连一次同一个目录下的库，用来模拟另一个进程。
pub async fn connect_again(dir: &tempfile::TempDir) -> DatabaseConnection {
    let url = format!("sqlite://{}?mode=rw", dir.path().join("test.db").display());
    connect(&url).await
}

async fn connect(url: &str) -> DatabaseConnection {
    let mut options = ConnectOptions::new(url.to_owned());
    options
        .max_connections(8)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(8));
    Database::connect(options).await.expect("connect sqlite")
}
