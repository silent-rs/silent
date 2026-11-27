//! `dev` 在第一次启动应用之前做的事，对应 `docs/design.md` §12 的 L0：
//!
//! - 有待执行的迁移时提示对应的命令，不代劳；
//! - `--migrate` 时先执行一次 `migrate up`，失败就不启动应用，并告诉页面；
//! - 把页面脚本的地址通过 `SB_DEV_RELOAD_URL` 传给应用，开发构建的应用会把它注入 HTML。
//!
//! 这里的"应用"是一个脚本：子命令按 silent-boot 的约定回答，启动时把要知道的东西写进 `log.txt`。
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use silent_boot_cli::dev::{BoxFuture, Builder, DevLoop, Startup};
use silent_boot_cli::events::Events;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

struct JustRun(PathBuf);

impl Builder for JustRun {
    fn bin_path(&self) -> PathBuf {
        self.0.clone()
    }

    fn build(&self) -> BoxFuture<'_, Result<PathBuf, String>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

const SCRIPT_URL: &str = "http://127.0.0.1:9230/__dev__/reload.js";

fn fake_app(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let script = dir.join("app.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
case "$1 $2" in
  "migrate status") echo '[{"scope":"framework","applied":[],"pending":["m1"]},{"scope":"app","applied":[],"pending":["m2","m3"]}]'; exit 0 ;;
  "migrate up") echo "up" >> log.txt; [ -f fail-up ] && exit 1; exit 0 ;;
esac
echo "run $SB_DEV_RELOAD_URL" >> log.txt
echo $$ > pid.txt
exec sleep 60
"#,
    )
    .expect("script");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    (script, dir.join("log.txt"), dir.join("pid.txt"))
}

fn read_log(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn wait_for(log: &Path, lines: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entries = read_log(log);
        if entries.len() >= lines || Instant::now() > deadline {
            return entries;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn process_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .status()
        .is_ok_and(|status| status.success())
}

/// 起一个只监听空目录的运行循环，返回停掉它的发送端和事件服务器。
async fn start(
    dir: &Path,
    script: PathBuf,
    startup: Startup,
) -> (oneshot::Sender<()>, Arc<Events>) {
    let events = Arc::new(
        Events::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind"),
    );
    let (stop, stopped) = oneshot::channel::<()>();
    let loop_events = events.clone();
    let cwd = dir.to_path_buf();
    tokio::spawn(async move {
        // 只监听一个永远是空的目录：这里不测重启。
        let idle = cwd.join("idle");
        std::fs::create_dir_all(&idle).expect("idle dir");
        let mut watcher =
            silent_boot_cli::watch::SourceWatcher::start(&[idle], &[]).expect("watcher");
        DevLoop::new(JustRun(script), cwd, loop_events)
            .envs(vec![(
                "SB_DEV_RELOAD_URL".to_owned(),
                SCRIPT_URL.to_owned(),
            )])
            .startup(startup)
            .run(watcher.changes(), async {
                let _ = stopped.await;
            })
            .await;
    });
    (stop, events)
}

async fn stop_and_wait_app(dir: &Path, stop: oneshot::Sender<()>) {
    stop.send(()).expect("stop the loop");
    let pid_file = dir.join("pid.txt");
    if let Ok(pid) = std::fs::read_to_string(&pid_file) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while process_alive(pid.trim()) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!process_alive(pid.trim()), "the app must be stopped");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_migrations_are_hinted_and_the_script_url_is_passed_to_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (script, log, pid_file) = fake_app(dir.path());
    let (stop, _events) = start(dir.path(), script, Startup::HINT).await;
    let entries = wait_for(&log, 1).await;
    assert_eq!(entries, vec![format!("run {SCRIPT_URL}")], "{entries:?}");
    assert!(pid_file.exists());
    stop_and_wait_app(dir.path(), stop).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dash_migrate_runs_migrate_up_before_the_app_starts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (script, log, _) = fake_app(dir.path());
    let (stop, _events) = start(dir.path(), script, Startup::MIGRATE).await;
    let entries = wait_for(&log, 2).await;
    assert_eq!(
        entries,
        vec!["up".to_owned(), format!("run {SCRIPT_URL}")],
        "{entries:?}"
    );
    stop_and_wait_app(dir.path(), stop).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_migration_keeps_the_app_from_starting_and_tells_the_page() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (script, log, pid_file) = fake_app(dir.path());
    std::fs::write(dir.path().join("fail-up"), "").expect("fail-up");
    let (stop, events) = start(dir.path(), script, Startup::MIGRATE).await;
    let mut page = TcpStream::connect(events.addr()).await.expect("connect");
    page.write_all(b"GET /__dev__/events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .expect("send");
    let mut text = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !text.contains("build-error") && Instant::now() < deadline {
        let mut chunk = [0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(2), page.read(&mut chunk))
            .await
            .unwrap_or(Ok(0))
            .unwrap_or(0);
        if n == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&chunk[..n]));
    }
    assert!(text.contains("event: build-error"), "{text}");
    assert!(text.contains("migrate up failed"), "{text}");

    let entries = wait_for(&log, 1).await;
    assert_eq!(entries, vec!["up".to_owned()], "the app must not start");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!pid_file.exists(), "the app must not start");
    assert_eq!(read_log(&log).len(), 1);
    stop.send(()).expect("stop the loop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_startup_options_nothing_runs_before_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (script, log, _) = fake_app(dir.path());
    let (stop, _events) = start(dir.path(), script, Startup::NONE).await;
    let entries = wait_for(&log, 1).await;
    assert_eq!(entries, vec![format!("run {SCRIPT_URL}")], "{entries:?}");
    stop_and_wait_app(dir.path(), stop).await;
}
